-- Writes a new lesson from one of Swara's templates into the current folder, with copies of the
-- includes it needs beside it, so the lesson builds with plain `lilypond` and travels as a folder.

local M = {}

-- The plugin's folder, three levels above this file.
local ROOT = vim.fn.fnamemodify(debug.getinfo(1, 'S').source:sub(2), ':p:h:h:h')

-- The includes each kind of lesson needs beside it.
local INCLUDES = {
  guitar = { 'swara-sargam.ily' },
  piano = { 'swara-sargam.ily' },
  song = { 'swara-sargam.ily', 'swara-lyrics.ily' },
}

-- The LilyPond version a lesson names when the installed one is not known.
local VERSION = '2.26.0'

local USAGE = 'Swara: usage: :Swara new guitar|piano|song <name>'

local function warn(message)
  vim.notify(message, vim.log.levels.WARN)
end

--- The bytes of the file at `path`, or nil and why it could not be read.
local function read(path)
  local file, err = io.open(path, 'rb')
  if not file then
    return nil, err
  end
  local text = file:read('a')
  file:close()
  return text
end

--- `text` with each `{{title}}` replaced by `name` and each `{{version}}` by `version`.
local function fill(text, name, version)
  return (text:gsub('{{(%w+)}}', { title = name, version = version }))
end

--- Writes `text` to `path` unless a file is already there. Returns true when it wrote, false when a
--- file was already there, and nil with the reason when it could not write.
local function create(path, text)
  local fd, err, code = vim.uv.fs_open(path, 'wx', tonumber('644', 8))
  if not fd then
    if code == 'EEXIST' then
      return false
    end
    return nil, err
  end
  local written, write_err = vim.uv.fs_write(fd, text)
  vim.uv.fs_close(fd)
  if not written then
    return nil, write_err
  end
  return true
end

--- Writes the lesson `<name>.ly` of `kind` (guitar, piano or song) in the current folder, with
--- `swara-sargam.ily` beside it, `swara-lyrics.ily` and `<name>-lyrics.ily` too for a song. An
--- include already there is kept as it is. A lesson already there stops everything. Calls `done`
--- with the lesson's path once it is written.
function M.new(kind, name, done)
  name = vim.trim(name or '')
  if not INCLUDES[kind] or name == '' then
    warn(USAGE)
    return
  end
  if name:find('[/\\"%c]') then
    warn(('Swara: a lesson name cannot hold /, \\, " or a control character: %s'):format(name))
    return
  end
  local dir = vim.fn.getcwd()
  local lesson = name .. '.ly'
  if vim.uv.fs_stat(dir .. '/' .. lesson) then
    warn(('Swara: %s is already there, so nothing was written'):format(lesson))
    return
  end

  -- Each file to write, as { name, source in the plugin, whether its placeholders are filled },
  -- the lesson first so that one already there stops the rest.
  local files = { { lesson, 'templates/' .. kind .. '.ly', true } }
  for _, include in ipairs(INCLUDES[kind]) do
    table.insert(files, { include, 'includes/' .. include, false })
  end
  if kind == 'song' then
    table.insert(files, { name .. '-lyrics.ily', 'templates/song-lyrics.ily', true })
  end
  for _, file in ipairs(files) do
    local text, err = read(ROOT .. '/' .. file[2])
    if not text then
      warn('Swara: a template is missing from the plugin: ' .. err)
      return
    end
    file.text = text
  end

  require('swara.binary').resolve(function(found)
    local lilypond = found and type(found.info) == 'table' and found.info.lilypond
    local version = type(lilypond) == 'table' and type(lilypond.version) == 'string' and lilypond.version or VERSION
    local wrote, kept = {}, {}
    for i, file in ipairs(files) do
      local ok, err = create(dir .. '/' .. file[1], file[3] and fill(file.text, name, version) or file.text)
      if ok == nil then
        warn(('Swara: could not write %s: %s'):format(file[1], err))
        return
      elseif ok then
        table.insert(wrote, file[1])
      elseif i == 1 then
        warn(('Swara: %s is already there, so nothing was written'):format(lesson))
        return
      else
        table.insert(kept, file[1])
      end
    end
    local message = 'Swara: wrote ' .. table.concat(wrote, ', ')
    if #kept > 0 then
      message = message .. ', and kept the ' .. table.concat(kept, ', ') .. ' already there'
    end
    vim.notify(message, vim.log.levels.INFO)
    if done then
      done(dir .. '/' .. lesson)
    end
  end)
end

return M
