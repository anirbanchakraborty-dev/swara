local h = require('helpers')

local M = {}

-- The `swara info` answer most tests give, with a LilyPond version no template carries itself.
local INFO = { lilypond = { version = '2.24.4' } }

--- Runs `templates.new(kind, name)` in a child Neovim whose current folder is `dir`, where the
--- search for the `swara` program answers with `info`, or fails when `info` is nil. Returns the
--- messages it showed.
local function new(dir, kind, name, info)
  return h.child_json(([[
    vim.fn.chdir(%q)
    local messages = {}
    vim.notify = function(msg)
      table.insert(messages, msg)
    end
    local info = vim.json.decode(%q)
    package.loaded['swara.binary'] = {
      resolve = function(callback)
        if info == vim.NIL then
          return callback(nil, 'no swara')
        end
        callback({ path = 'swara', source = 'path', info = info })
      end,
    }
    require('swara.templates').new(%s, %s)
    print(vim.json.encode({ messages = messages }))
  ]]):format(dir, vim.json.encode(info == nil and vim.NIL or info), vim.inspect(kind), vim.inspect(name))).messages
end

--- The names in `dir`, sorted.
local function files(dir)
  local names = {}
  for name in vim.fs.dir(dir) do
    table.insert(names, name)
  end
  table.sort(names)
  return names
end

local function said(messages, text)
  local all = table.concat(messages, '\n')
  assert(all:find(text, 1, true), ('no message names %q: %s'):format(text, all))
end

function M.guitar_creates_files()
  local dir = h.tmpdir()
  local messages = new(dir, 'guitar', 'scales', INFO)
  assert(vim.deep_equal(files(dir), { 'scales.ly', 'swara-sargam.ily' }), vim.inspect(files(dir)))
  local lesson = h.read(dir .. '/scales.ly')
  assert(lesson:find('title = "scales"', 1, true), lesson)
  assert(lesson:find('\\version "2.24.4"', 1, true), lesson)
  assert(not lesson:find('{{', 1, true), lesson)
  assert(h.read(dir .. '/swara-sargam.ily') == h.read(h.root .. '/includes/swara-sargam.ily'))
  said(messages, 'scales.ly')
end

function M.version_falls_back()
  for _, info in ipairs({ vim.NIL, { lilypond = vim.NIL }, {} }) do
    local dir = h.tmpdir()
    new(dir, 'piano', 'etude', info ~= vim.NIL and info or nil)
    local lesson = h.read(dir .. '/etude.ly')
    assert(lesson:find('\\version "2.26.0"', 1, true), vim.inspect(info) .. ': ' .. lesson)
  end
end

function M.existing_lesson_untouched()
  local dir = h.tmpdir()
  h.write(dir .. '/scales.ly', 'my own scales\n')
  local messages = new(dir, 'guitar', 'scales', INFO)
  assert(h.read(dir .. '/scales.ly') == 'my own scales\n')
  assert(vim.deep_equal(files(dir), { 'scales.ly' }), vim.inspect(files(dir)))
  said(messages, 'scales.ly')
end

function M.edited_include_kept()
  local dir = h.tmpdir()
  h.write(dir .. '/swara-sargam.ily', '% edited by hand\n')
  local messages = new(dir, 'piano', 'etude', INFO)
  assert(h.read(dir .. '/swara-sargam.ily') == '% edited by hand\n')
  assert(h.read(dir .. '/etude.ly'), 'no etude.ly')
  said(messages, 'swara-sargam.ily')
end

function M.name_with_space()
  local dir = h.tmpdir()
  new(dir, 'song', 'my song', INFO)
  local want = { 'my song-lyrics.ily', 'my song.ly', 'swara-lyrics.ily', 'swara-sargam.ily' }
  assert(vim.deep_equal(files(dir), want), vim.inspect(files(dir)))
  local lesson = h.read(dir .. '/my song.ly')
  assert(lesson:find('\\include "my song-lyrics.ily"', 1, true), lesson)
  assert(h.read(dir .. '/swara-lyrics.ily') == h.read(h.root .. '/includes/swara-lyrics.ily'))
  assert(not h.read(dir .. '/my song-lyrics.ily'):find('{{', 1, true))
end

function M.missing_name_shows_usage()
  local dir = h.tmpdir()
  for _, args in ipairs({ { 'guitar', nil }, { 'guitar', '' }, { 'guitar', '  ' }, { 'violin', 'scales' }, {} }) do
    said(new(dir, args[1], args[2], INFO), ':Swara new')
  end
  assert(#files(dir) == 0, vim.inspect(files(dir)))
end

function M.name_with_path_characters_refused()
  local dir = h.tmpdir()
  for _, name in ipairs({ 'a/b', 'a\\b', 'a"b' }) do
    said(new(dir, 'guitar', name, INFO), name)
  end
  assert(#files(dir) == 0, vim.inspect(files(dir)))
end

function M.templates_build_clean()
  if vim.fn.executable('lilypond') ~= 1 then
    return
  end
  local dir = h.tmpdir()
  for _, kind in ipairs({ 'guitar', 'piano', 'song' }) do
    new(dir, kind, kind .. ' lesson', nil)
  end
  local function build(file)
    local result = vim
      .system({ 'lilypond', '-l', 'WARNING', '-o', dir .. '/out', dir .. '/' .. file }, {
        text = true,
        env = { LC_ALL = 'C' },
      })
      :wait(120000)
    assert(result.code == 0 and result.stderr == '', ('%s exited %d: %s'):format(file, result.code, result.stderr))
  end
  build('guitar lesson.ly')
  build('piano lesson.ly')
  local song = h.read(dir .. '/song lesson.ly')
  for _, lines in ipairs({ 'both', 'script', 'roman' }) do
    local text, n = song:gsub("lyricLines = #'both", "lyricLines = #'" .. lines)
    assert(n == 1, song)
    h.write(dir .. '/song lesson.ly', text)
    build('song lesson.ly')
  end
end

return M
