-- Playback through FluidSynth, and the hand-off to MuseScore. Each test runs in its own Neovim,
-- because the player the plugin starts lives for the rest of the session. Fake `fluidsynth` and
-- `swara` programs log their pid and arguments. The fake FluidSynth reads its standard input as the
-- real one does, keeps each line it is sent in a second log, and answers `echo` the same way, so the
-- tests see what reached FluidSynth through the pipe the plugin gives it.

local h = require('helpers')

-- Logs its pid and arguments, then reads commands until its input ends, as FluidSynth's shell does:
-- a prompt, the line read back, and for `echo` the words after it.
local FLUIDSYNTH = [[
echo "fluidsynth $$ $*" >> "$SWARA_TEST_LOG"
printf '> '
while IFS= read -r line; do
  echo "$line" >> "$SWARA_TEST_LOG.in"
  echo "$line"
  case "$line" in
    'echo '*) echo "${line#echo }" ;;
  esac
  printf '> '
done]]

-- The same, but it takes half a second to answer `echo`, as the real one takes over a second to
-- load the soundfont and open its audio output.
local SLOW_FLUIDSYNTH = FLUIDSYNTH:gsub([['echo '%*%) echo]], "'echo '*) sleep 0.5; echo")

-- Answers `swara info` as the plugin's lookup needs it, and for `play` logs, sends FluidSynth a note
-- through its output and waits to be killed under the same pid.
local SWARA = [[
case "$1" in
  info) echo '{"version":"0.0.0"}' ;;
  play) echo "swara $$ $*" >> "$SWARA_TEST_LOG"; echo 'noteon 0 60 100'; exec sleep 60 ;;
esac]]

local PRELUDE = [=[
local h = require('helpers')
local playback = require('swara.playback')
local PID = vim.fn.getpid()
local SOUNDFONT = h.write(h.tmpdir() .. '/test.sf2', 'soundfont\n')
require('swara.config').setup({ soundfont = SOUNDFONT })

local messages = {}
vim.notify = function(message)
  table.insert(messages, message)
end

local function out(value)
  io.stdout:write(vim.json.encode(value), '\n')
end

--- What the fake `name` logged each time it started, as { pid, args }, oldest first.
local function logged(name)
  local found = {}
  if vim.fn.filereadable(vim.env.SWARA_TEST_LOG) == 1 then
    for _, line in ipairs(vim.fn.readfile(vim.env.SWARA_TEST_LOG)) do
      local program, pid, args = line:match('^(%S+) (%d+) (.*)$')
      if program == name then
        table.insert(found, { pid = tonumber(pid), args = args })
      end
    end
  end
  return found
end

--- Each line the fake FluidSynth was sent, oldest first.
local function received()
  local path = vim.env.SWARA_TEST_LOG .. '.in'
  return vim.fn.filereadable(path) == 1 and vim.fn.readfile(path) or {}
end

local function alive(pid)
  return vim.uv.kill(pid, 0) == 0
end

--- Waits until `name` has started `count` times, and says whether it did.
local function until_started(name, count)
  return vim.wait(5000, function()
    return #logged(name) >= count
  end, 10)
end

--- Waits until FluidSynth has been sent `line`, and says whether it was.
local function until_received(line)
  return vim.wait(5000, function()
    return vim.tbl_contains(received(), line)
  end, 10)
end

--- Holds every thread of libuv's pool with reads that wait for data, so that a write the plugin
--- queues there cannot finish until the function returned lets them go. The pool has four threads
--- unless `UV_THREADPOOL_SIZE` says otherwise, and sixteen reads cover either.
local function hold_pool()
  local pipes = {}
  for i = 1, 16 do
    local fds = assert(vim.uv.pipe())
    vim.uv.fs_read(fds.read, 1, -1, function()
      vim.uv.fs_close(fds.read)
    end)
    pipes[i] = fds
  end
  return function()
    for _, fds in ipairs(pipes) do
      vim.uv.fs_write(fds.write, 'x', -1)
      vim.uv.fs_close(fds.write)
    end
  end
end

local buf = vim.api.nvim_get_current_buf()
local MIDI = h.tmpdir() .. '/score.midi'
vim.b[buf].swara_midi = MIDI
]=]

--- Puts the fake programs first on `PATH` and gives them a fresh log. `fakes` maps each name to
--- its script, and defaults to both fakes above.
local function prepare(fakes)
  vim.env.SWARA_TEST_LOG = h.tmpdir() .. '/started.log'
  for name, script in pairs(fakes or { fluidsynth = FLUIDSYNTH, swara = SWARA }) do
    h.fake_bin(name, script)
  end
end

--- Runs `body` after PRELUDE in a child Neovim and returns the JSON it printed last.
local function run(body, fakes)
  prepare(fakes)
  return h.child_json(PRELUDE .. body)
end

local MUSESCORE = [=[
local facts = vim.json.decode(%q)
local runs, messages = {}, {}
local has = vim.fn.has
vim.fn.has = function(feature)
  if feature == 'mac' then
    return facts.mac and 1 or 0
  end
  return has(feature)
end
vim.fn.executable = function(name)
  return facts.programs[name] and 1 or 0
end
vim.system = function(cmd, _, on_exit)
  table.insert(runs, cmd)
  if on_exit then
    on_exit({ code = facts.code, signal = 0, stdout = '', stderr = facts.stderr })
  end
  return {}
end
vim.notify = function(message)
  table.insert(messages, message)
end
local buf = vim.api.nvim_get_current_buf()
vim.b[buf].swara_midi = facts.midi
require('swara.musescore').open(buf)
vim.wait(20)
io.stdout:write(vim.json.encode({ runs = runs, messages = messages }), '\n')
]=]

local MIDI = '/scores/Āmār sonār bāṅlā.midi'

--- Opens MuseScore in a child Neovim whose platform, programs and `vim.system` answer as `facts`
--- says, and returns the commands it ran and the messages it gave.
local function musescore(facts)
  facts = vim.tbl_extend('keep', facts, { mac = true, programs = {}, code = 0, stderr = '', midi = MIDI })
  return h.child_json(MUSESCORE:format(vim.json.encode(facts)))
end

local tests = {}

function tests.first_play_starts_fluidsynth_once()
  local result = run([[
    playback.play(buf)
    local played = until_started('swara', 1)
    out({ played = played, fluidsynth = #logged('fluidsynth'), swara = logged('swara'), midi = MIDI, messages = messages })
  ]])
  assert(result.played, 'no `swara play`: ' .. vim.inspect(result.messages))
  assert(result.fluidsynth == 1, result.fluidsynth .. ' FluidSynths started')
  assert(result.swara[1].args == 'play ' .. result.midi, result.swara[1].args)
  assert(#result.messages == 0, vim.inspect(result.messages))
end

function tests.fluidsynth_is_started_without_a_port()
  -- FluidSynth opens a network port only when told to with `-s`, and its port listens on every
  -- network interface, so the plugin drives its standard input instead.
  local result = run([[
    playback.play(buf)
    until_started('swara', 1)
    out({ args = logged('fluidsynth')[1].args, expected = '-o synth.dynamic-sample-loading=1 ' .. SOUNDFONT })
  ]])
  assert(result.args == result.expected, result.args)
end

function tests.swara_play_writes_to_fluidsynth()
  local result = run([[
    playback.play(buf)
    until_started('swara', 1)
    local reached = until_received('noteon 0 60 100')
    out({ reached = reached, lines = received() })
  ]])
  assert(result.reached, 'FluidSynth was sent ' .. vim.inspect(result.lines))
  assert(result.lines[1] == 'echo swara-ready', vim.inspect(result.lines))
end

function tests.play_waits_until_fluidsynth_answers()
  -- FluidSynth reads its input back before it answers, so the question alone is not the answer.
  local result = run(
    [[
    playback.play(buf)
    local asked = until_received('echo swara-ready')
    vim.wait(250)
    local early = #logged('swara')
    local played = until_started('swara', 1)
    out({ asked = asked, early = early, played = played })
  ]],
    { fluidsynth = SLOW_FLUIDSYNTH, swara = SWARA }
  )
  assert(result.asked, 'FluidSynth was not asked whether it is ready')
  assert(result.early == 0, '`swara play` started before FluidSynth answered')
  assert(result.played, 'no `swara play` once FluidSynth answered')
end

function tests.second_play_reuses_it()
  local result = run([[
    playback.play(buf)
    until_started('swara', 1)
    playback.play(buf)
    local played = until_started('swara', 2)
    out({ played = played, fluidsynth = #logged('fluidsynth') })
  ]])
  assert(result.played, 'no second `swara play`')
  assert(result.fluidsynth == 1, result.fluidsynth .. ' FluidSynths started')
end

function tests.play_twice_stops_first()
  local result = run([[
    playback.play(buf)
    until_started('swara', 1)
    local first = logged('swara')[1].pid
    local spawn, first_alive = vim.uv.spawn, nil
    vim.uv.spawn = function(path, options, ...)
      if options.args and options.args[1] == 'play' then
        first_alive = alive(first)
      end
      return spawn(path, options, ...)
    end
    playback.play(buf)
    local played = until_started('swara', 2)
    out({ played = played, first_alive = first_alive })
  ]])
  assert(result.played, 'no second `swara play`')
  assert(result.first_alive == false, 'the first `swara play` was running when the second started')
end

function tests.second_play_waits_for_the_silence()
  -- The silence goes out on libuv's pool. Held there, it must still reach FluidSynth before the
  -- next stream's first line, or that stream plays at zero gain.
  local result = run([[
    playback.play(buf)
    until_started('swara', 1)
    until_received('noteon 0 60 100')
    local release = hold_pool()
    playback.play(buf)
    vim.wait(300)
    release()
    until_started('swara', 2)
    vim.wait(5000, function() return #received() >= 5 end, 10)
    out({ lines = received() })
  ]])
  local expected = { 'echo swara-ready', 'noteon 0 60 100', 'gain 0', 'reset', 'noteon 0 60 100' }
  assert(vim.deep_equal(result.lines, expected), vim.inspect(result.lines))
end

function tests.play_after_a_stop_waits_for_the_silence()
  local result = run([[
    playback.play(buf)
    until_started('swara', 1)
    playback.stop()
    vim.wait(5000, function() return #received() >= 4 end, 10)
    local release = hold_pool()
    playback.stop()
    playback.play(buf)
    vim.wait(300)
    release()
    until_started('swara', 2)
    vim.wait(5000, function() return #received() >= 7 end, 10)
    out({ lines = received() })
  ]])
  local expected = { 'echo swara-ready', 'noteon 0 60 100', 'gain 0', 'reset', 'gain 0', 'reset', 'noteon 0 60 100' }
  assert(vim.deep_equal(result.lines, expected), vim.inspect(result.lines))
end

function tests.stop_while_loading()
  local result = run(
    [[
    playback.play(buf)
    until_started('fluidsynth', 1)
    playback.stop()
    local asked = until_received('echo swara-ready')
    -- Past the fake's half-second answer.
    vim.wait(800)
    local streams = #logged('swara')
    playback.play(buf)
    until_started('swara', 1)
    out({ asked = asked, streams = streams, fluidsynth = #logged('fluidsynth'), messages = messages })
  ]],
    { fluidsynth = SLOW_FLUIDSYNTH, swara = SWARA }
  )
  assert(result.asked, 'the player stopped loading')
  assert(result.streams == 0, 'a `swara play` started after the stop')
  assert(result.fluidsynth == 1, 'the next play started another FluidSynth')
  assert(#result.messages == 0, vim.inspect(result.messages))
end

function tests.stop_sends_gain_zero_then_reset()
  local result = run([[
    playback.play(buf)
    until_started('swara', 1)
    local pid = logged('swara')[1].pid
    playback.stop()
    local sent = vim.wait(3000, function() return received()[#received()] == 'reset' end, 10)
    out({ sent = sent, lines = received(), playing = alive(pid), messages = messages })
  ]])
  assert(result.sent, 'FluidSynth was sent ' .. vim.inspect(result.lines))
  local n = #result.lines
  assert(result.lines[n - 1] == 'gain 0' and result.lines[n] == 'reset', vim.inspect(result.lines))
  assert(result.playing == false, '`swara play` is still running')
  assert(#result.messages == 0, vim.inspect(result.messages))
end

function tests.quit_leaves_nothing()
  prepare()
  local result = h.child(PRELUDE .. [[
    playback.play(buf)
    assert(until_started('swara', 1), 'no `swara play`')
    vim.cmd('qall!')
  ]])
  assert(result.code == 0, ('child exited %d: %s%s'):format(result.code, result.stdout, result.stderr))
  local pids = {}
  for _, line in ipairs(vim.fn.readfile(vim.env.SWARA_TEST_LOG)) do
    table.insert(pids, tonumber(line:match('^%S+ (%d+)')))
  end
  assert(#pids == 2, vim.inspect(pids))
  for _, pid in ipairs(pids) do
    local gone = h.wait_for(function()
      return vim.uv.kill(pid, 0) ~= 0
    end, 3000)
    assert(gone, ('pid %d is still running'):format(pid))
  end
end

function tests.fluidsynth_ends_when_neovim_is_killed()
  -- A Neovim that is killed runs no exit handler. FluidSynth still ends, because the end of its input
  -- closes with the last process that holds it, and a `swara play` holds it only while it plays.
  local quick = [[
case "$1" in
  info) echo '{"version":"0.0.0"}' ;;
  play) echo "swara $$ $*" >> "$SWARA_TEST_LOG"; echo 'noteon 0 60 100' ;;
esac]]
  prepare({ fluidsynth = FLUIDSYNTH, swara = quick })
  h.child(PRELUDE .. [[
    playback.play(buf)
    assert(until_started('swara', 1), 'no `swara play`')
    assert(until_received('noteon 0 60 100'), 'FluidSynth was sent ' .. vim.inspect(received()))
    vim.wait(100)
    vim.uv.kill(PID, 'sigkill')
  ]])
  local started = vim.fn.readfile(vim.env.SWARA_TEST_LOG)[1]
  local pid = tonumber(started:match('^fluidsynth (%d+)'))
  assert(pid, started)
  local gone = h.wait_for(function()
    return vim.uv.kill(pid, 0) ~= 0
  end, 3000)
  assert(gone, ('FluidSynth, pid %d, is still running'):format(pid))
end

function tests.no_build_yet()
  local result = run([[
    vim.b[buf].swara_midi = nil
    playback.play(buf)
    vim.wait(100)
    out({ messages = messages, fluidsynth = #logged('fluidsynth') })
  ]])
  assert(result.messages[1] == 'Swara: build the file first (save it)', vim.inspect(result.messages))
  assert(result.fluidsynth == 0, 'FluidSynth started')
end

function tests.soundfont_defaults_to_ms_basic()
  local result = run([[
    require('swara.config').setup({})
    local readable = vim.fn.filereadable
    vim.fn.filereadable = function(path)
      return path:find('MS Basic.sf3', 1, true) and 1 or readable(path)
    end
    playback.play(buf)
    until_started('fluidsynth', 1)
    out({ args = logged('fluidsynth')[1].args })
  ]])
  local ms_basic = '/Applications/MuseScore 4.app/Contents/Resources/sound/MS Basic.sf3'
  assert(vim.endswith(result.args, ' ' .. ms_basic), result.args)
end

function tests.no_soundfont_says_so()
  local result = run([[
    require('swara.config').setup({})
    local readable = vim.fn.filereadable
    vim.fn.filereadable = function(path)
      return path:find('MS Basic.sf3', 1, true) and 0 or readable(path)
    end
    playback.play(buf)
    vim.wait(100)
    out({ messages = messages, fluidsynth = #logged('fluidsynth') })
  ]])
  assert(result.messages[1] == "Swara: set `soundfont` in Swara's options", vim.inspect(result.messages))
  assert(result.fluidsynth == 0, 'FluidSynth started')
end

function tests.soundfont_that_is_not_there_says_so()
  local result = run([[
    require('swara.config').setup({ soundfont = '/nowhere/piano.sf2' })
    playback.play(buf)
    vim.wait(100)
    out({ messages = messages, fluidsynth = #logged('fluidsynth') })
  ]])
  assert(result.messages[1] == 'Swara: soundfont not found: /nowhere/piano.sf2', vim.inspect(result.messages))
  assert(result.fluidsynth == 0, 'FluidSynth started')
end

function tests.fluidsynth_not_installed_says_so()
  local result = run(
    [[
    vim.env.PATH = vim.fn.fnamemodify(vim.fn.exepath('swara'), ':h') .. ':/usr/bin:/bin'
    playback.play(buf)
    vim.wait(100)
    out({ messages = messages })
  ]],
    { swara = SWARA }
  )
  assert(result.messages[1] == 'Swara: FluidSynth not found on PATH', vim.inspect(result.messages))
end

function tests.fluidsynth_that_fails_is_reported()
  local failing = 'echo "fluidsynth $$ $*" >> "$SWARA_TEST_LOG"\necho "fluidsynth: error: no audio device" >&2\nexit 1'
  local result = run(
    [[
    playback.play(buf)
    vim.wait(5000, function() return #messages > 0 end, 10)
    vim.wait(100)
    out({ messages = messages, fluidsynth = #logged('fluidsynth'), swara = #logged('swara') })
  ]],
    { fluidsynth = failing, swara = SWARA }
  )
  assert(result.fluidsynth == 1, result.fluidsynth .. ' FluidSynths started')
  assert(result.swara == 0, 'a `swara play` started')
  local expected = 'Swara: FluidSynth did not start: fluidsynth: error: no audio device'
  assert(result.messages[1] == expected, vim.inspect(result.messages))
end

function tests.stream_that_fails_is_reported()
  local failing = [[
case "$1" in
  info) echo '{"version":"0.0.0"}' ;;
  play) echo "swara: cannot read score.midi" >&2; exit 1 ;;
esac]]
  local result = run(
    [[
    playback.play(buf)
    vim.wait(5000, function() return #messages > 0 end, 10)
    out({ messages = messages })
  ]],
    { fluidsynth = FLUIDSYNTH, swara = failing }
  )
  assert(result.messages[1] == 'Swara: playback failed: swara: cannot read score.midi', vim.inspect(result.messages))
end

function tests.player_that_exits_is_started_again()
  local result = run([[
    playback.play(buf)
    until_started('swara', 1)
    local first = logged('fluidsynth')[1].pid
    vim.uv.kill(first, 'sigterm')
    vim.wait(2000, function() return not alive(first) end, 10)
    vim.wait(50)
    playback.play(buf)
    local played = until_started('swara', 2)
    out({ played = played, fluidsynth = #logged('fluidsynth'), messages = messages })
  ]])
  assert(result.played, 'no second `swara play`')
  assert(result.fluidsynth == 2, result.fluidsynth .. ' FluidSynths started')
end

function tests.musescore_opens_the_midi_on_mac()
  local result = musescore({ mac = true })
  assert(vim.deep_equal(result.runs, { { 'open', '-a', 'MuseScore 4', MIDI } }), vim.inspect(result.runs))
  assert(#result.messages == 0, vim.inspect(result.messages))
end

function tests.musescore_failure_on_mac_is_reported()
  local result = musescore({ mac = true, code = 1, stderr = "Unable to find application named 'MuseScore 4'\n" })
  local expected = "Swara: MuseScore did not open: Unable to find application named 'MuseScore 4'"
  assert(result.messages[1] == expected, vim.inspect(result.messages))
end

function tests.musescore_elsewhere_runs_the_first_found()
  local result = musescore({ mac = false, programs = { mscore = true, musescore = true } })
  assert(vim.deep_equal(result.runs, { { 'mscore', MIDI } }), vim.inspect(result.runs))
end

function tests.musescore_not_found_says_so()
  local result = musescore({ mac = false })
  local expected = 'Swara: MuseScore not found (looked for mscore4, mscore and musescore)'
  assert(result.messages[1] == expected, vim.inspect(result.messages))
  assert(#result.runs == 0, vim.inspect(result.runs))
end

function tests.musescore_before_a_build_says_so()
  local result = musescore({ midi = vim.NIL })
  assert(result.messages[1] == 'Swara: build the file first (save it)', vim.inspect(result.messages))
  assert(#result.runs == 0, vim.inspect(result.runs))
end

-- Times real playback for `docs/bench/speed.md`: the real FluidSynth with MuseScore's soundfont,
-- and the `swara` that `cargo build` leaves in `target/debug`. It plays a scale out loud, and checks
-- that the FluidSynth it starts holds no network socket.
function tests.real_play_times()
  if vim.env.SWARA_REAL ~= '1' then
    return
  end
  local dir = h.tmpdir()
  h.write(
    dir .. '/score.ly',
    [[
\version "2.26.0"
\score {
  \relative { c'4 d e f g a b c }
  \layout { }
  \midi { }
}
]]
  )
  local built = vim.system({ 'lilypond', '-o', dir .. '/score', dir .. '/score.ly' }, { text = true }):wait(60000)
  assert(built.code == 0, built.stderr)
  vim.env.PATH = h.root .. '/target/debug:' .. vim.env.PATH
  local result = h.child_json(
    ([=[
    local playback = require('swara.playback')
    local buf = vim.api.nvim_get_current_buf()
    vim.b[buf].swara_midi = %q
    local spawn, spawned = vim.uv.spawn, {}
    vim.uv.spawn = function(path, options, ...)
      if options.args and options.args[1] == 'play' then
        table.insert(spawned, vim.uv.hrtime())
      end
      return spawn(path, options, ...)
    end
    local function timed(count)
      local start = vim.uv.hrtime()
      playback.play(buf)
      assert(vim.wait(10000, function() return #spawned >= count end, 1), 'no `swara play`')
      return (spawned[count] - start) / 1e6
    end
    local first = timed(1)
    local fluidsynth = vim.trim(vim.fn.system({ 'pgrep', '-P', tostring(vim.uv.os_getpid()), '-x', 'fluidsynth' }))
    local sockets = vim.trim(vim.fn.system({ 'lsof', '-nP', '-a', '-p', fluidsynth, '-i' }))
    vim.wait(1500)
    local second = timed(2)
    vim.wait(500)
    playback.stop()
    vim.wait(100)
    io.stdout:write(vim.json.encode({ first = first, second = second, fluidsynth = fluidsynth, sockets = sockets }), '\n')
  ]=]):format(dir .. '/score.midi'),
    60000
  )
  assert(result.fluidsynth:match('^%d+$'), 'no FluidSynth found: ' .. result.fluidsynth)
  assert(result.sockets == '', 'FluidSynth holds a network socket:\n' .. result.sockets)
  io.stdout:write(('real_play_times: first play %.1f ms, second play %.1f ms\n'):format(result.first, result.second))
end

return tests
