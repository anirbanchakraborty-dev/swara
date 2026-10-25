-- Plays the MIDI file of a buffer's last build through FluidSynth, by the route chosen in
-- `docs/spikes/playback.md`. FluidSynth is started once, with its command shell on a local port,
-- and kept running with the soundfont loaded. Each play is a `swara play` process that streams the
-- file's events to that port in time.

local config = require('swara.config')

local M = {}

-- MuseScore 4's bundled General MIDI soundfont, used when the options name none.
local MS_BASIC = '/Applications/MuseScore 4.app/Contents/Resources/sound/MS Basic.sf3'

-- How often a refused connection to a starting player is tried again.
local RETRY_MS = 50
-- How long a program that took the connection may stay silent before its port counts as taken.
local SILENT_MS = 2000
-- How long FluidSynth may take to open its port. It opened after 1,056 ms in the spike.
local OPEN_MS = 10000
-- How many ports are tried, upward from the first, before FluidSynth counts as unable to start.
local PORTS = 8

local PID = vim.uv.os_getpid()
local FIRST_PORT = 9800 + PID % 200

-- The FluidSynth being started, as { proc, port, token, tcp, tries, soundfont, stderr, started }.
local loading
-- The FluidSynth that answered with its token, the same table, whose `tcp` now carries the stops.
local player
-- The `swara play` process streaming to the player, as { proc }.
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

--- Silences the player at once: `gain 0`, then `reset`, because `reset` alone left sound for
--- 122 ms on the song and 580 ms on a held organ note. The next `swara play` sets the gain again.
local function silence()
  if player and not player.tcp:is_closing() then
    player.tcp:write('gain 0\nreset\n')
  end
end

local launch, advance

--- Called when the `swara play` process `this` exits. A stream told to stop lets the player be
--- silenced, now that nothing it sent can still arrive, and lets the next play start.
local function ended(this, result)
  if stream == this then
    stream = nil
    if result.code ~= 0 then
      warn('Swara: playback failed: ' .. vim.trim(result.stderr or ''))
    end
  elseif dying == this then
    dying = nil
    silence()
    advance()
  end
end

--- Starts the play asked for once nothing stands in its way. An older stream still exiting calls
--- this again when it has, and so does a player that is still starting, once it is ready.
function advance()
  if not wanted or stream or dying then
    return
  end
  if not player then
    if not loading then
      launch(wanted.soundfont, 0)
    end
    return
  end
  local play = wanted
  wanted = nil
  local this = {}
  local cmd = { play.exe, 'play', '--port', tostring(player.port), play.midi }
  local ok, proc = pcall(vim.system, cmd, { text = true, stdout = false }, function(result)
    vim.schedule(function()
      ended(this, result)
    end)
  end)
  if not ok then
    return warn('Swara: playback failed: ' .. tostring(proc))
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
  close(this.tcp)
  kill(this.proc)
  wanted = nil
  warn(message)
end

--- Gives up the port of `this` player: kills the FluidSynth started for it and tries the next port
--- up, or reports the failure once `PORTS` ports have been tried.
local function busy(this)
  if loading ~= this then
    return
  end
  if this.tries + 1 < PORTS then
    loading = nil
    close(this.tcp)
    kill(this.proc)
    return launch(this.soundfont, this.tries + 1)
  end
  local last = vim.trim(this.stderr):match('[^\n]*$')
  local message = ('Swara: FluidSynth did not start on ports %d to %d'):format(FIRST_PORT, this.port)
  fail(this, last ~= '' and (message .. ': ' .. last) or message)
end

--- Called when the FluidSynth of `this` exits. One still starting counts its port as taken, and
--- one that was ready is forgotten, so that the next play starts another.
local function exited(this)
  if loading == this then
    busy(this)
  elseif player == this then
    close(this.tcp)
    player = nil
  end
end

--- Takes the bytes the port of `this` sends. The port is the player's own only when the first
--- bytes are its token. Once it is, everything after is a prompt, read and thrown away.
local function received(this, err, data)
  if player == this then
    if err or not data then
      close(this.tcp)
      kill(this.proc)
      player = nil
    end
    return
  end
  if loading ~= this then
    return
  end
  if err or not data then
    return busy(this)
  end
  this.got = this.got .. data
  local n = math.min(#this.got, #this.token)
  if this.got:sub(1, n) ~= this.token:sub(1, n) then
    return busy(this)
  end
  if n == #this.token then
    loading, player = nil, this
    advance()
  end
end

--- Connects to the port of `this` player, again every `RETRY_MS` while the connection is refused.
local function probe(this)
  if loading ~= this then
    return
  end
  if vim.uv.now() - this.started > OPEN_MS then
    return fail(this, ('Swara: FluidSynth did not open port %d within %d s'):format(this.port, OPEN_MS / 1000))
  end
  local tcp = vim.uv.new_tcp()
  this.tcp = tcp
  tcp:connect('127.0.0.1', this.port, function(err)
    vim.schedule(function()
      if loading ~= this then
        return close(tcp)
      end
      if err then
        close(tcp)
        return vim.defer_fn(function()
          probe(this)
        end, RETRY_MS)
      end
      this.got = ''
      vim.defer_fn(function()
        busy(this)
      end, SILENT_MS)
      tcp:read_start(function(read_err, data)
        vim.schedule(function()
          received(this, read_err, data)
        end)
      end)
    end)
  end)
end

--- Starts FluidSynth on the port `tries` above the first, exactly as the spike gives it, with a
--- prompt that names this process and the port so that no other program can pass for it.
function launch(soundfont, tries)
  local port = FIRST_PORT + tries
  local token = ('swara-%d-%d>'):format(PID, port)
  local this = { port = port, token = token, tries = tries, soundfont = soundfont, stderr = '' }
  loading = this
  local cmd = {
    'fluidsynth',
    '-s',
    '-i',
    '-o',
    'shell.port=' .. port,
    '-o',
    'shell.prompt=' .. token,
    '-o',
    'synth.dynamic-sample-loading=1',
    soundfont,
  }
  local ok, proc = pcall(vim.system, cmd, {
    stdout = false,
    stderr = function(_, data)
      if data then
        vim.schedule(function()
          this.stderr = this.stderr .. data
          if data:find('bind', 1, true) then
            busy(this)
          end
        end)
      end
    end,
  }, function()
    vim.schedule(function()
      exited(this)
    end)
  end)
  if not ok then
    return fail(this, 'Swara: FluidSynth could not be started: ' .. tostring(proc))
  end
  this.proc = proc
  this.started = vim.uv.now()
  probe(this)
end

--- The soundfont to play with, or nil and the reason there is none.
local function soundfont()
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
--- a session starts FluidSynth, which takes about a second, and later plays reuse it.
function M.play(buf)
  local midi = vim.b[buf].swara_midi
  if type(midi) ~= 'string' then
    vim.notify('Swara: build the file first (save it)', vim.log.levels.INFO)
    return
  end
  local font, reason = soundfont()
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
  end,
})

return M
