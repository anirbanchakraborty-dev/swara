local h = require('helpers')

local T = {}

local function set_lines(buf, lines)
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
end

--- Attaches Swara's grammar to `buf` and parses it, so that captures can be read at once.
local function attach(buf)
  require('swara.treesitter').attach(buf, h.info())
  assert(vim.treesitter.highlighter.active[buf], 'highlighting did not start')
  vim.treesitter.get_parser(buf, 'lilypond'):parse()
end

--- The capture names at a 0-based row and column.
local function captures(buf, row, col)
  return vim.tbl_map(function(capture)
    return capture.capture
  end, vim.treesitter.get_captures_at_pos(buf, row, col))
end

local function assert_capture(buf, row, col, name)
  local found = captures(buf, row, col)
  assert(vim.tbl_contains(found, name), ('expected @%s at %d:%d, got %s'):format(name, row, col, vim.inspect(found)))
end

function T.highlights_start(buf)
  set_lines(buf, { [[\relative c' { c4\f d }]] })
  attach(buf)
  -- `\relative` is a command, which the grammar's mapping colours as a function call.
  assert_capture(buf, 0, 0, 'function.call')
  -- `\f` is a dynamic.
  assert_capture(buf, 0, 17, 'attribute.builtin')
  assert(vim.wo[0][0].foldexpr == 'v:lua.vim.treesitter.foldexpr()', 'foldexpr is ' .. vim.wo.foldexpr)
end

function T.other_grammar_refused()
  local out = h.child_json([[
    local h = require('helpers')
    local messages = {}
    vim.notify = function(message)
      table.insert(messages, message)
    end
    local c = vim.api.nvim_get_runtime_file('parser/c.*', false)[1]
    assert(c, 'Neovim has no bundled C parser')
    assert(vim.treesitter.language.add('lilypond', { path = c, symbol_name = 'c' }))
    local info = h.info()
    local bufs = { vim.api.nvim_create_buf(true, true), vim.api.nvim_create_buf(true, true) }
    local started = false
    for _, buf in ipairs(bufs) do
      vim.api.nvim_set_current_buf(buf)
      require('swara.treesitter').attach(buf, info)
      started = started or vim.treesitter.highlighter.active[buf] ~= nil or vim.bo[buf].indentexpr ~= ''
    end
    io.stdout:write(vim.json.encode({ started = started, messages = messages }), '\n')
  ]])
  assert(not out.started, 'Swara started on the C grammar')
  assert(#out.messages == 1, 'expected one message, got ' .. vim.inspect(out.messages))
  assert(out.messages[1]:find(':checkhealth swara', 1, true), out.messages[1])
end

function T.other_queries_refused()
  -- Neovim reads only the first `highlights.scm` for a language on the runtime path, so another
  -- plugin's LilyPond queries earlier on it would be run on Swara's grammar.
  local dir = h.tmpdir()
  h.write(dir .. '/queries/lilypond/highlights.scm', '(no_such_node) @variable\n')
  local out = h.child_json(([[
    vim.opt.runtimepath:prepend(%q)
    local h = require('helpers')
    local messages = {}
    vim.notify = function(message)
      table.insert(messages, message)
    end
    local buf = vim.api.nvim_create_buf(true, true)
    vim.api.nvim_set_current_buf(buf)
    require('swara.treesitter').attach(buf, h.info())
    local started = vim.treesitter.highlighter.active[buf] ~= nil or vim.bo[buf].indentexpr ~= ''
    io.stdout:write(vim.json.encode({ started = started, messages = messages }), '\n')
  ]]):format(dir))
  assert(not out.started, 'Swara started with the other queries')
  assert(#out.messages == 1, 'expected one message, got ' .. vim.inspect(out.messages))
  local message = out.messages[1]
  assert(message:find(':checkhealth swara', 1, true), message)
  assert(message:find('no_such_node', 1, true), message)
  assert(not message:find('\n', 1, true), 'the message is more than one line: ' .. message)
end

function T.indent_inside_blocks(buf)
  set_lines(buf, { [[\score {]], '{', 'c4 d', '}', '}' })
  attach(buf)
  assert(
    vim.bo[buf].indentexpr == "v:lua.require'swara.indent'.get(v:lnum)",
    'indentexpr is ' .. vim.bo[buf].indentexpr
  )
  vim.bo[buf].expandtab = true
  vim.bo[buf].shiftwidth = 3
  vim.cmd('silent normal! gg=G')
  local want = { [[\score {]], '   {', '      c4 d', '   }', '}' }
  local got = vim.api.nvim_buf_get_lines(buf, 0, -1, false)
  assert(vim.deep_equal(got, want), vim.inspect(got))
  -- A new line under a block that is not closed yet, as when typing it.
  set_lines(buf, { [[\score {]], '' })
  assert(require('swara.indent').get(2) == 3, 'blank line under an open block')
end

function T.indent_matches_formatter(buf)
  -- The formatter tidies on save, so `=` lays a score out as it does, `<< {` included.
  local lines = {
    [[\header {]],
    [[title = "Two voices"]],
    [[}]],
    [[\score {]],
    [[\new Staff \with {]],
    [[instrumentName = "Voice"]],
    [[} <<]],
    [[{]],
    [[c4 d]],
    [[} \\ {]],
    [[e4 f]],
    [[}]],
    [[>>]],
    [[<< {]],
    [[g4]],
    [[} \\ {]],
    [[a4]],
    [[} >>]],
    [[\layout { }]],
    [[}]],
  }
  local exe = h.root .. '/target/debug/swara'
  local tidy = vim.system({ exe, 'format', '--indent', '2' }, { stdin = table.concat(lines, '\n') .. '\n' }):wait()
  assert(tidy.code == 0, tidy.stderr)
  set_lines(buf, lines)
  attach(buf)
  vim.bo[buf].expandtab = true
  vim.bo[buf].shiftwidth = 2
  vim.cmd('silent normal! gg=G')
  local got = vim.api.nvim_buf_get_lines(buf, 0, -1, false)
  local want = vim.split(tidy.stdout, '\n', { trimempty = true })
  assert(vim.deep_equal(got, want), ('got %s\nwant %s'):format(vim.inspect(got), vim.inspect(want)))
end

function T.unclosed_brace_keeps_colours(buf)
  set_lines(buf, {
    [[\version "2.26.0"]],
    [[\language "english"]],
    [[\header {]],
    [[  title = "Unclosed"]],
    [[}]],
    [[\score {]],
    [[  <<]],
    [[    \new Staff \relative c' {]],
    [[      \clef treble]],
    [[      c4 d e f |]],
    [[      g4 a b c |]],
    [[      d4 c b a |]],
    [[      g4 f e d |]],
    [[      c1 \bar "|."]],
    [[    }]],
    [[  >>]],
    [[  \layout { }]],
    [[  \midi { }]],
    [[  % the end]],
    [[}]],
  })
  attach(buf)
  assert_capture(buf, 9, 6, 'variable')
  vim.api.nvim_buf_set_text(buf, 19, 0, 19, 1, { '' })
  vim.treesitter.get_parser(buf, 'lilypond'):parse()
  assert_capture(buf, 9, 6, 'variable')
end

function T.colours_under_16ms(buf)
  set_lines(buf, vim.fn.readfile(h.root .. '/bench/song-100.ly'))
  attach(buf)
  local parser = vim.treesitter.get_parser(buf, 'lilypond')
  local times = {}
  for i = 1, 20 do
    local row = 100 + i
    local start = vim.uv.hrtime()
    vim.api.nvim_buf_set_text(buf, row, 0, row, 0, { ' ' })
    parser:parse()
    table.insert(times, (vim.uv.hrtime() - start) / 1e6)
  end
  table.sort(times)
  local median = (times[10] + times[11]) / 2
  io.stdout:write(('  colours_under_16ms: median %.2f ms\n'):format(median))
  assert(median < 16, ('median %.2f ms'):format(median))
end

return T
