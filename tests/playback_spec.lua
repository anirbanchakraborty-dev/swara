-- Playback through FluidSynth, and the hand-off to MuseScore. Each test runs in its own Neovim,
-- because the player the plugin starts lives for the rest of the session. Fake `fluidsynth` and
-- `swara` programs log their pid and arguments, and the test stands in for FluidSynth's shell with
-- a TCP server on the port, which greets each client with the player's token and records each line.
-- That server answers only once the fake FluidSynth for its port has logged, as a real port cannot
-- answer before FluidSynth runs, so that every FluidSynth the plugin started is in the log.

local h = require('helpers')

-- Logs its pid and arguments, then waits to be killed under the same pid.
local FLUIDSYNTH = 'echo "fluidsynth $$ $*" >> "$SWARA_TEST_LOG"\nexec sleep 60'

-- Answers `swara info` as the plugin's lookup needs it, and for `play` logs and waits like
-- FLUIDSYNTH.
local SWARA = [[
case "$1" in
  info) echo '{"version":"0.0.0"}' ;;
  play) echo "swara $$ $*" >> "$SWARA_TEST_LOG"; exec sleep 60 ;;
esac]]

local PRELUDE = [=[
local h = require('helpers')
local playback = require('swara.playback')
local PID = vim.fn.getpid()
local BASE = 9800 + PID % 200
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

local function alive(pid)
  return vim.uv.kill(pid, 0) == 0
end

--- Waits until `name` has started `count` times, and says whether it did.
local function until_started(name, count)
  return vim.wait(5000, function()
    return #logged(name) >= count
  end, 10)
end

-- What each stand-in shell was sent, and how many clients it took, by port.
local received, clients = {}, {}

--- Stands in for FluidSynth's shell on `port`. It sends `greeting`, by default the player's token,
--- to each client that connects, once the fake FluidSynth for `port` has logged, and again after
--- each line, as the shell sends its prompt.
local function serve(port, greeting)
  greeting = greeting or ('swara-%d-%d>'):format(PID, port)
  received[port], clients[port] = {}, 0
  local server = vim.uv.new_tcp()
  local listening = server:bind('127.0.0.1', port)
    and server:listen(8, function()
      local client = vim.uv.new_tcp()
      server:accept(client)
      clients[port] = clients[port] + 1
      local function greet()
        if client:is_closing() then
          return
        end
        for _, started in ipairs(logged('fluidsynth')) do
          if started.args:find('shell.port=' .. port .. ' ', 1, true) then
            return client:write(greeting)
          end
        end
        vim.defer_fn(greet, 10)
      end
      vim.schedule(greet)
      local rest = ''
      client:read_start(function(_, data)
        if not data then
          return client:close()
        end
        rest = rest .. data
        for line in rest:gmatch('([^\n]*)\n') do
          table.insert(received[port], line)
          client:write(greeting)
        end
        rest = rest:match('[^\n]*$')
      end)
    end)
  assert(listening, ('port %d is taken'):format(port))
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
    serve(BASE)
    playback.play(buf)
    local played = until_started('swara', 1)
    out({ played = played, fluidsynth = #logged('fluidsynth'), swara = logged('swara'), port = BASE, midi = MIDI, messages = messages })
  ]])
  assert(result.played, 'no `swara play`: ' .. vim.inspect(result.messages))
  assert(result.fluidsynth == 1, result.fluidsynth .. ' FluidSynths started')
  local expected = ('play --port %d %s'):format(result.port, result.midi)
  assert(result.swara[1].args == expected, result.swara[1].args)
  assert(#result.messages == 0, vim.inspect(result.messages))
end

function tests.second_play_reuses_it()
  local result = run([[
    serve(BASE)
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
    serve(BASE)
    playback.play(buf)
    until_started('swara', 1)
    local first = logged('swara')[1].pid
    local spawn, first_alive = vim.system, nil
    vim.system = function(cmd, ...)
      if cmd[2] == 'play' then
        first_alive = alive(first)
      end
      return spawn(cmd, ...)
    end
    playback.play(buf)
    local played = until_started('swara', 2)
    out({ played = played, first_alive = first_alive })
  ]])
  assert(result.played, 'no second `swara play`')
  assert(result.first_alive == false, 'the first `swara play` was running when the second started')
end

function tests.stop_while_loading()
  local result = run([[
    playback.play(buf)
    until_started('fluidsynth', 1)
    playback.stop()
    serve(BASE)
    local reached = vim.wait(3000, function() return clients[BASE] > 0 end, 10)
    vim.wait(200)
    local streams = #logged('swara')
    playback.play(buf)
    until_started('swara', 1)
    out({ reached = reached, streams = streams, fluidsynth = #logged('fluidsynth'), messages = messages })
  ]])
  assert(result.reached, 'the player stopped loading')
  assert(result.streams == 0, 'a `swara play` started after the stop')
  assert(result.fluidsynth == 1, 'the next play started another FluidSynth')
  assert(#result.messages == 0, vim.inspect(result.messages))
end

function tests.stop_sends_gain_zero_then_reset()
  local result = run([[
    serve(BASE)
    playback.play(buf)
    until_started('swara', 1)
    local pid = logged('swara')[1].pid
    playback.stop()
    local sent = vim.wait(3000, function() return #received[BASE] >= 2 end, 10)
    out({ sent = sent, lines = received[BASE], playing = alive(pid), messages = messages })
  ]])
  assert(result.sent, 'the shell was sent ' .. vim.inspect(result.lines))
  local n = #result.lines
  assert(result.lines[n - 1] == 'gain 0' and result.lines[n] == 'reset', vim.inspect(result.lines))
  assert(result.playing == false, '`swara play` is still running')
  assert(#result.messages == 0, vim.inspect(result.messages))
end

function tests.port_answered_by_another_program_is_skipped()
  local result = run([[
    serve(BASE, 'hello')
    serve(BASE + 1)
    playback.play(buf)
    local played = until_started('swara', 1)
    local fluidsynths = logged('fluidsynth')
    local first_gone = vim.wait(2000, function() return not alive(fluidsynths[1].pid) end, 10)
    out({ played = played, fluidsynths = fluidsynths, first_gone = first_gone, swara = logged('swara'), port = BASE })
  ]])
  assert(result.played, 'no `swara play`')
  assert(#result.fluidsynths == 2, #result.fluidsynths .. ' FluidSynths started')
  assert(result.fluidsynths[1].args:find('shell.port=' .. result.port .. ' ', 1, true), result.fluidsynths[1].args)
  assert(result.fluidsynths[2].args:find('shell.port=' .. result.port + 1 .. ' ', 1, true), result.fluidsynths[2].args)
  assert(result.first_gone, 'the FluidSynth for the taken port is still running')
  assert(result.swara[1].args:find('--port ' .. result.port + 1 .. ' ', 1, true), result.swara[1].args)
end

function tests.fluidsynth_is_launched_with_its_token()
  local result = run([[
    serve(BASE)
    playback.play(buf)
    until_started('fluidsynth', 1)
    out({
      args = logged('fluidsynth')[1].args,
      expected = ('-s -i -o shell.port=%d -o shell.prompt=swara-%d-%d> -o synth.dynamic-sample-loading=1 %s'):format(
        BASE, PID, BASE, SOUNDFONT
      ),
    })
  ]])
  assert(result.args == result.expected, result.args)
end

function tests.quit_leaves_nothing()
  prepare()
  local result = h.child(PRELUDE .. [[
    serve(BASE)
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
    serve(BASE)
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

function tests.fluidsynth_that_keeps_failing_is_reported()
  local failing = 'echo "fluidsynth $$ $*" >> "$SWARA_TEST_LOG"\necho "fluidsynth: error: no audio device" >&2\nexit 1'
  local result = run(
    [[
    playback.play(buf)
    vim.wait(5000, function() return #messages > 0 end, 10)
    out({ messages = messages, fluidsynth = #logged('fluidsynth'), port = BASE })
  ]],
    { fluidsynth = failing, swara = SWARA }
  )
  assert(result.fluidsynth == 8, result.fluidsynth .. ' FluidSynths started')
  local expected = ('Swara: FluidSynth did not start on ports %d to %d: fluidsynth: error: no audio device'):format(
    result.port,
    result.port + 7
  )
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
    serve(BASE)
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
    serve(BASE)
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
-- and the `swara` that `cargo build` leaves in `target/debug`. It plays a scale out loud.
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
    local spawn, spawned = vim.system, {}
    vim.system = function(cmd, ...)
      if cmd[2] == 'play' then
        table.insert(spawned, vim.uv.hrtime())
      end
      return spawn(cmd, ...)
    end
    local function timed(count)
      local start = vim.uv.hrtime()
      playback.play(buf)
      assert(vim.wait(10000, function() return #spawned >= count end, 1), 'no `swara play`')
      return (spawned[count] - start) / 1e6
    end
    local first = timed(1)
    vim.wait(1500)
    local second = timed(2)
    vim.wait(500)
    playback.stop()
    vim.wait(100)
    io.stdout:write(vim.json.encode({ first = first, second = second }), '\n')
  ]=]):format(dir .. '/score.midi'),
    60000
  )
  io.stdout:write(('real_play_times: first play %.1f ms, second play %.1f ms\n'):format(result.first, result.second))
end

return tests
