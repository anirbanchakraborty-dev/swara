-- Opens the MIDI file of a buffer's last build in MuseScore, for slower practice with its tempo
-- and loop controls.

local M = {}

-- The names MuseScore's program goes by outside macOS, newest first.
local PROGRAMS = { 'mscore4', 'mscore', 'musescore' }

local function warn(message)
  vim.notify(message, vim.log.levels.WARN)
end

--- Where MuseScore is: MuseScore 4's application on macOS, and elsewhere the first MuseScore
--- program on `PATH`. Nil when it is not there.
function M.find()
  if vim.fn.has('mac') == 1 then
    for _, dir in ipairs({ '/Applications', vim.fs.normalize('~/Applications') }) do
      if vim.fn.isdirectory(dir .. '/MuseScore 4.app') == 1 then
        return dir .. '/MuseScore 4.app'
      end
    end
    return nil
  end
  for _, name in ipairs(PROGRAMS) do
    if vim.fn.executable(name) == 1 then
      return vim.fn.exepath(name)
    end
  end
  return nil
end

--- Opens the MIDI file of `buf`'s last build in MuseScore 4 on macOS, and elsewhere in the first
--- MuseScore program found on `PATH`.
function M.open(buf)
  local midi = vim.b[buf].swara_midi
  if type(midi) ~= 'string' then
    vim.notify('Swara: build the file first (save it)', vim.log.levels.INFO)
    return
  end
  if vim.fn.has('mac') == 1 then
    -- `open` returns once MuseScore has the file, and says why when it cannot.
    vim.system({ 'open', '-a', 'MuseScore 4', midi }, { text = true }, function(result)
      if result.code ~= 0 then
        vim.schedule(function()
          warn('Swara: MuseScore did not open: ' .. vim.trim(result.stderr))
        end)
      end
    end)
    return
  end
  for _, name in ipairs(PROGRAMS) do
    if vim.fn.executable(name) == 1 then
      -- MuseScore itself runs until it is closed, so it is detached and its output is not kept.
      local ok, err = pcall(vim.system, { name, midi }, { detach = true, stdout = false, stderr = false })
      if not ok then
        warn('Swara: MuseScore did not open: ' .. tostring(err))
      end
      return
    end
  end
  warn('Swara: MuseScore not found (looked for mscore4, mscore and musescore)')
end

return M
