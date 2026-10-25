local h = require('helpers')

local T = {}

local exe = h.root .. '/target/debug/swara'

--- Runs `body` in a child Neovim where Swara's server is set up with `opts` and `text` is open as
--- `score.ly`. `body` sees `buf`, `opened` (the `hrtime` before the file was opened) and
--- `client()`, and returns a table, which comes back decoded with every message shown added as
--- `messages`. Also returns the file's path.
local function with_server(opts, text, body)
  local dir = h.tmpdir()
  local file = h.write(dir .. '/score.ly', text)
  -- A LilyPond that does not exist, so that a save tidies the file and builds nothing.
  opts = vim.tbl_extend('keep', opts, { lilypond = dir .. '/no-lilypond' })
  local out = h.child_json(([[
    local h = require('helpers')
    local messages = {}
    vim.notify = function(message)
      table.insert(messages, message)
    end
    require('swara').setup(%s)
    require('swara.lsp').setup({ path = %q, source = 'path', info = h.info() })
    local opened = vim.uv.hrtime()
    vim.cmd.edit(%q)
    local buf = vim.api.nvim_get_current_buf()
    require('swara.format').attach(buf)
    local function client()
      return vim.lsp.get_clients({ bufnr = buf, name = 'swara' })[1]
    end
    assert(h.wait_for(client, 5000), 'the server did not start')
    local result = (function()
      %s
    end)()
    result.messages = messages
    io.stdout:write(vim.json.encode(result), '\n')
  ]]):format(vim.inspect(opts), exe, file, body))
  return out, file
end

--- Asserts that nothing was shown but the server's notice that the missing LilyPond the tests
--- give it is missing, which arrives at no fixed time.
local function assert_quiet(out)
  local others = vim.tbl_filter(function(message)
    return not message:find('LilyPond not found', 1, true)
  end, out.messages)
  assert(#others == 0, 'expected no messages, got ' .. vim.inspect(others))
end

function T.syntax_error_shows()
  local out = with_server(
    {},
    '{ c d\n',
    [[
    local found = h.wait_for(function()
      for _, diagnostic in ipairs(vim.diagnostic.get(buf)) do
        if diagnostic.message == 'missing `}`' then
          return true
        end
      end
      return false
    end, 1000)
    return { found = found, ms = (vim.uv.hrtime() - opened) / 1e6 }
  ]]
  )
  assert(out.found, 'no "missing `}`" diagnostic')
  assert(out.ms < 1000, ('the diagnostic took %d ms'):format(out.ms))
end

function T.debounce_is_10ms()
  local out = with_server({}, '{ c4 }\n', 'return { debounce = client().flags.debounce_text_changes }')
  assert(out.debounce == 10, 'debounce_text_changes is ' .. tostring(out.debounce))
end

function T.tidy_on_save()
  local out, file = with_server(
    {},
    '{ c4  d |e }\n',
    [[
    vim.cmd('silent write')
    return {}
  ]]
  )
  assert(h.read(file) == '{ c4 d | e }\n', vim.inspect(h.read(file)))
  assert_quiet(out)
end

function T.tidy_off()
  local out, file = with_server(
    { format_on_save = false },
    '{ c4  d |e }\n',
    [[
    vim.cmd('silent write')
    return {}
  ]]
  )
  assert(h.read(file) == '{ c4  d |e }\n', vim.inspect(h.read(file)))
  assert_quiet(out)
end

function T.conform_takes_over()
  -- conform.nvim loads late in many setups, so it is looked for at each save, not at attach.
  local out, file = with_server(
    {},
    '{ c4  d |e }\n',
    [[
    package.loaded.conform = {
      formatters_by_ft = { lilypond = { 'fake' } },
      list_formatters_to_run = function(bufnr)
        assert(bufnr == buf, 'conform was asked about buffer ' .. tostring(bufnr))
        return { { name = 'fake', available = true } }, false
      end,
    }
    vim.cmd('silent write')
    return {}
  ]]
  )
  assert(h.read(file) == '{ c4  d |e }\n', vim.inspect(h.read(file)))
  assert_quiet(out)
end

function T.conform_without_formatter_tidies()
  local out, file = with_server(
    {},
    '{ c4  d |e }\n',
    [[
    package.loaded.conform = {
      list_formatters_to_run = function()
        return {}, false
      end,
    }
    vim.cmd('silent write')
    return {}
  ]]
  )
  assert(h.read(file) == '{ c4 d | e }\n', vim.inspect(h.read(file)))
  assert_quiet(out)
end

function T.conform_catch_all_still_tidies()
  -- A formatter conform runs on every filetype, such as a whitespace trimmer under `*` or `_`, is
  -- not a LilyPond formatter, so Swara still tidies.
  local out, file = with_server(
    {},
    '{ c4  d |e }\n',
    [[
    package.loaded.conform = {
      formatters_by_ft = { ['*'] = { 'trim_whitespace' }, _ = { 'trim_newlines' } },
      list_formatters_to_run = function()
        return { { name = 'trim_whitespace', available = true } }, false
      end,
    }
    vim.cmd('silent write')
    return {}
  ]]
  )
  assert(h.read(file) == '{ c4 d | e }\n', vim.inspect(h.read(file)))
  assert_quiet(out)
end

function T.build_notification_stored()
  local out = with_server(
    {},
    '{ c4 }\n',
    [[
    local seen
    package.loaded['swara.viewer'] = {
      built = function(built, params)
        seen = { same_buffer = built == buf, pdf = params.pdf }
      end,
    }
    local handler = client().handlers['swara/buildFinished']
    local ctx = { client_id = client().id, method = 'swara/buildFinished' }
    handler(nil, {
      uri = vim.uri_from_bufnr(buf),
      pdf = '/out/score.pdf',
      midi = '/out/score.midi',
      success = true,
      durationMs = 812,
    }, ctx)
    -- A build of a file that is not open touches no buffer and makes none.
    local before = #vim.api.nvim_list_bufs()
    handler(nil, { uri = vim.uri_from_fname('/elsewhere/other.ly'), success = true, durationMs = 5 }, ctx)
    return {
      pdf = vim.b[buf].swara_pdf,
      midi = vim.b[buf].swara_midi,
      seen = seen,
      new_buffers = #vim.api.nvim_list_bufs() - before,
    }
  ]]
  )
  assert(out.pdf == '/out/score.pdf', 'swara_pdf is ' .. tostring(out.pdf))
  assert(out.midi == '/out/score.midi', 'swara_midi is ' .. tostring(out.midi))
  assert(out.seen and out.seen.same_buffer and out.seen.pdf == '/out/score.pdf', vim.inspect(out.seen))
  assert(out.new_buffers == 0, 'the other build made a buffer')
  assert_quiet(out)
end

function T.opening_attaches()
  -- Through `plugin/swara.lua`, with the built `swara` first on `PATH`. Both files are open before
  -- the search for `swara` ends, as they are when it downloads, and both get everything.
  local dir = h.tmpdir()
  local a = h.write(dir .. '/a.ly', '{ c4 }\n')
  local b = h.write(dir .. '/b.ly', '{ d4 }\n')
  local out = h.child_json(([[
    local h = require('helpers')
    local messages = {}
    vim.notify = function(message)
      table.insert(messages, message)
    end
    vim.env.PATH = %q .. ':' .. vim.env.PATH
    require('swara').setup({ lilypond = %q })
    vim.cmd.runtime('plugin/swara.lua')
    vim.cmd.edit(%q)
    local a = vim.api.nvim_get_current_buf()
    vim.cmd.edit(%q)
    local b = vim.api.nvim_get_current_buf()
    local function ready(buf)
      return vim.treesitter.highlighter.active[buf] ~= nil
        and vim.lsp.get_clients({ bufnr = buf, name = 'swara' })[1] ~= nil
        and #vim.api.nvim_get_autocmds({ event = 'BufWritePre', buffer = buf }) > 0
    end
    h.wait_for(function()
      return ready(a) and ready(b)
    end, 5000)
    io.stdout:write(vim.json.encode({
      a = ready(a),
      b = ready(b),
      servers = #vim.lsp.get_clients({ name = 'swara' }),
      messages = messages,
    }), '\n')
  ]]):format(h.root .. '/target/debug', dir .. '/no-lilypond', a, b))
  assert(out.a and out.b, vim.inspect(out))
  assert(out.servers == 1, 'expected one server, got ' .. out.servers)
  assert_quiet(out)
end

return T
