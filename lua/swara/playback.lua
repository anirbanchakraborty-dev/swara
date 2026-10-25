-- Plays the MIDI file of a buffer's last build through FluidSynth. FluidSynth is started once and
-- kept running with the soundfont loaded, reading commands from a pipe this plugin holds open as its
-- standard input. Each play is a `swara play` process whose output is that same pipe, so it writes
-- the file's events straight to FluidSynth in time. FluidSynth opens no network port this way. Its
-- own command port listens on every network interface, which is why the plugin does not use it.
-- `docs/spikes/playback.md` has the measurements behind both routes.

local config = require('swara.config')

local M = {}

-- MuseScore 4's bundled General MIDI soundfont, used when the options name none.
local MS_BASIC = '/Applications/MuseScore 4.app/Contents/Resources/sound/MS Basic.sf3'

-- How long FluidSynth may take to answer once started. It answered after about 1,550 ms with
-- MuseScore's soundfont.
local OPEN_MS = 10000
-- What FluidSynth is asked to say back. It reads its input only once the soundfont is loaded and its
-- audio output is open, so the answer means a note sent now is heard.
local READY = 'swara-ready'
-- How much of a program's error output is kept for a message, from the end.
local STDERR_KEEP = 4096

-- The FluidSynth being started, as { proc, input, pending, closing }, where `input` is the end of its
-- input pipe this plugin writes to and `pending` counts the writes still under way.
local loading
-- The FluidSynth that answered, the same table.
local player
-- The `swara play` process writing to the player, as { proc }.
local stream
-- A stream that has been told to stop and has not exited yet.
local dying
-- The play asked for that has not started yet, as { exe, midi, soundfont }.
local wanted

local function warn(message)
  vim.notify(message, vim.log.levels.WARN)
end

local function kill(proc)
  if proc and not proc:is_closing() then
    pcall(proc.kill, proc, 'sigterm')
  end
end

local function close(handle)
  if handle and not handle:is_closing() then
    handle:close()
  end
end

--- Starts the program `path` with `args`. `stdin` and `stdout` are each a file descriptor, a pipe or
--- nil for none. Calls `on_exit({ code, stderr })` once the program has exited and its error output
--- has been read to the end. Returns the process, or nil and the reason it did not start.
local function spawn(path, args, stdin, stdout, on_exit)
  local errors = vim.uv.new_pipe(false)
  local stderr, code, drained = '', nil, false
  local function finish()
    if code and drained then
      vim.schedule(function()
        on_exit({ code = code, stderr = stderr })
      end)
    end
  end
  local proc, why
  proc, why = vim.uv.spawn(path, { args = args, stdio = { stdin, stdout, errors } }, function(status)
    code = status
    close(proc)
    finish()
  end)
  if not proc then
    close(errors)
    return nil, why
  end
  errors:read_start(function(err, data)
    if err or not data then
      close(errors)
      drained = true
      return finish()
    end
    stderr = (stderr .. data):sub(-STDERR_KEEP)
  end)
  return proc
end

local launch, advance

--- Writes `text` to the input of the FluidSynth `this`, on libuv's thread pool, so that a full pipe
--- never holds up the editor. Commands of up to 512 bytes reach FluidSynth whole even while a
--- `swara play` writes to the same pipe, because the system writes that much to a pipe at once.
--- The pool can be busy with other work, so a play waits for the last write to finish.
local function send(this, text)
  if this.closing then
    return
  end
  this.pending = this.pending + 1
  vim.uv.fs_write(this.input, text, -1, function()
    this.pending = this.pending - 1
    if this.pending > 0 then
      return
    end
    if this.closing then
      vim.uv.fs_close(this.input)
    else
      vim.schedule(advance)
    end
  end)
end

--- Closes the input of `this`, once the writes still under way have finished, so that the number is
--- never reused while one of them holds it. FluidSynth exits when its input ends.
local function shut(this)
  if this.closing then
    return
  end
  this.closing = true
  if this.pending == 0 then
    vim.uv.fs_close(this.input)
  end
end

--- Silences the player at once: `gain 0`, then `reset`, because `reset` alone left sound for
--- 122 ms on the song and 580 ms on a held organ note. The next `swara play` sets the gain again.
local function silence()
  if player then
    send(player, 'gain 0\nreset\n')
  end
end

--- Called when the `swara play` process `this` exits. A stream told to stop lets the player be
--- silenced, now that nothing it sent can still arrive, and lets the next play start.
local function ended(this, result)
  if stream == this then
    stream = nil
    if result.code ~= 0 then
      warn('Swara: playback failed: ' .. vim.trim(result.stderr))
    end
  elseif dying == this then
    dying = nil
    silence()
    advance()
  end
end

--- Starts the play asked for once nothing stands in its way. An older stream still exiting calls
--- this again when it has, a player that is still starting does once it is ready, and so does the
--- last write to the player once it has finished.
function advance()
  if not wanted or stream or dying then
    return
  end
  if not player then
    if not loading then
      launch(wanted.soundfont)
    end
    return
  end
  -- A silence still on its way would land after the new stream's first lines and mute it.
  if player.pending > 0 then
    return
  end
  local play = wanted
  wanted = nil
  local this = {}
  local proc, why = spawn(play.exe, { 'play', play.midi }, nil, player.input, function(result)
    ended(this, result)
  end)
  if not proc then
    return warn('Swara: playback failed: ' .. tostring(why))
  end
  this.proc = proc
  stream = this
end

--- Stops the stream, if one is playing. The player is silenced once the stream has exited.
local function halt()
  if stream then
    dying, stream = stream, nil
    kill(dying.proc)
  end
end

--- Gives up starting `this` player with `message`, and drops the play that was waiting for it.
local function fail(this, message)
  loading = nil
  kill(this.proc)
  shut(this)
  wanted = nil
  warn(message)
end

--- Called when the FluidSynth of `this` exits. One still starting is reported with the last line it
--- wrote, and one that was ready is forgotten, so that the next play starts another.
local function exited(this, result)
  if loading == this then
    local last = vim.trim(result.stderr):match('[^\n]*$')
    fail(this, last ~= '' and ('Swara: FluidSynth did not start: ' .. last) or 'Swara: FluidSynth did not start')
  elseif player == this then
    shut(this)
    player = nil
  end
end

--- Called once the FluidSynth of `this` has answered.
local function ready(this)
  if loading == this then
    loading, player = nil, this
    advance()
  end
end

--- Reads what the FluidSynth of `this` writes, until it answers with `READY` on a line of its own.
--- It reads each command back after its `> ` prompt before it answers, so the question is not the
--- answer. Everything after is read and thrown away, because FluidSynth would stop at a full pipe.
local function watch(this, output)
  local partial, answered = '', false
  output:read_start(function(err, data)
    if err or not data then
      return close(output)
    end
    if answered then
      return
    end
    partial = partial .. data
    for line in partial:gmatch('([^\n]*)\n') do
      while line:sub(1, 2) == '> ' do
        line = line:sub(3)
      end
      if vim.trim(line) == READY then
        answered = true
        return vim.schedule(function()
          ready(this)
        end)
      end
    end
    partial = partial:match('[^\n]*$')
  end)
end

--- Starts FluidSynth with `soundfont` and its input on a new pipe, and asks it to answer once it reads.
function launch(soundfont)
  local fds, why = vim.uv.pipe()
  if not fds then
    wanted = nil
    return warn('Swara: FluidSynth could not be started: ' .. tostring(why))
  end
  local output = vim.uv.new_pipe(false)
  local this = { input = fds.write, pending = 0 }
  loading = this
  local args = { '-o', 'synth.dynamic-sample-loading=1', soundfont }
  local proc
  proc, why = spawn('fluidsynth', args, fds.read, output, function(result)
    exited(this, result)
  end)
  -- FluidSynth holds its own copy now. Its input ends once every copy of the other end is closed.
  vim.uv.fs_close(fds.read)
  if not proc then
    close(output)
    return fail(this, 'Swara: FluidSynth could not be started: ' .. tostring(why))
  end
  this.proc = proc
  watch(this, output)
  send(this, 'echo ' .. READY .. '\n')
  vim.defer_fn(function()
    if loading == this then
      fail(this, ('Swara: FluidSynth did not start within %d s'):format(OPEN_MS / 1000))
    end
  end, OPEN_MS)
end

--- The soundfont to play with, or nil and the reason there is none.
function M.soundfont()
  local chosen = config.get().soundfont
  if chosen then
    local path = vim.fs.normalize(chosen)
    if vim.fn.filereadable(path) == 1 then
      return path
    end
    return nil, 'Swara: soundfont not found: ' .. path
  end
  if vim.fn.filereadable(MS_BASIC) == 1 then
    return MS_BASIC
  end
  return nil, "Swara: set `soundfont` in Swara's options"
end

--- Plays the MIDI file of `buf`'s last build, stopping whatever is playing first. The first play of
--- a session starts FluidSynth, which takes about a second and a half, and later plays reuse it.
function M.play(buf)
  local midi = vim.b[buf].swara_midi
  if type(midi) ~= 'string' then
    vim.notify('Swara: build the file first (save it)', vim.log.levels.INFO)
    return
  end
  local font, reason = M.soundfont()
  if not font then
    return warn(reason)
  end
  if vim.fn.executable('fluidsynth') ~= 1 then
    return warn('Swara: FluidSynth not found on PATH')
  end
  require('swara.binary').resolve(function(bin)
    -- A failed search has already said why, once.
    if not bin then
      return
    end
    wanted = { exe = bin.path, midi = midi, soundfont = font }
    halt()
    advance()
  end)
end

--- Stops playback. A player that is still starting goes on starting, so that the next play is quick.
function M.stop()
  wanted = nil
  halt()
  -- With a stream exiting, `ended` silences the player once it has gone.
  if not dying then
    silence()
  end
end

vim.api.nvim_create_autocmd('VimLeavePre', {
  group = vim.api.nvim_create_augroup('swara.playback', { clear = true }),
  desc = 'Stop Swara playback and FluidSynth',
  callback = function()
    for _, this in pairs({ stream, dying, loading, player }) do
      kill(this.proc)
    end
    for _, this in pairs({ loading, player }) do
      shut(this)
    end
  end,
})

return M
