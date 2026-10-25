local h = require('helpers')

local T = {}

local exe = h.root .. '/target/debug/swara'

local NOT_FOUND = 'LilyPond not found: the LilyPond check, builds, preview and playback are off'

-- Set up in each child before the test's body runs. `vim.system`, `vim.ui.open`, `vim.notify` and,
-- when `facts.server` is given, the Swara client record what they are asked to do instead of doing
-- it, and the machine looks as `facts` says.
local PRELUDE = [=[
local runs, opened, messages, requests = {}, {}, {}, {}
local has, isdirectory = vim.fn.has, vim.fn.isdirectory
vim.fn.has = function(feature)
  if feature == 'mac' then
    return facts.mac and 1 or 0
  end
  return has(feature)
end
vim.fn.isdirectory = function(path)
  if vim.endswith(path, '/Skim.app') then
    return facts.skim and 1 or 0
  end
  return isdirectory(path)
end
vim.system = function(cmd, opts, on_exit)
  if type(opts) == 'function' then
    opts, on_exit = nil, opts
  end
  local result = { code = 0, signal = 0, stdout = '', stderr = '' }
  if cmd[1] == 'defaults' then
    local value = facts.prefs[cmd[#cmd]]
    if value then
      result.stdout = value .. '\n'
    else
      result.code = 1
      result.stderr = "Error: Could not find key '" .. cmd[#cmd] .. "'\n"
    end
  else
    table.insert(runs, cmd)
    if cmd[1] == 'osascript' and facts.osascript ~= 0 then
      result.code = facts.osascript
      result.stderr = '83:98: execution error: Not authorized to send Apple events to Skim. (-1743)\n'
    end
  end
  if on_exit then
    on_exit(result)
  end
  return { pid = 1, kill = function() end, wait = function() return result end }
end
vim.ui.open = function(path)
  table.insert(opened, path)
  return { wait = function() return { code = 0 } end }, nil
end
vim.notify = function(message)
  table.insert(messages, message)
end
if facts.server ~= nil then
  vim.lsp.get_clients = function(filter)
    -- Neovim asks with no filter as it exits.
    if not facts.server or (filter or {}).name ~= 'swara' then
      return {}
    end
    return { {
      id = 1,
      name = 'swara',
      request = function(_, method, params, handler, bufnr)
        table.insert(requests, { method = method, params = params })
        handler(facts.server.err, facts.server.result, { method = method, client_id = 1, bufnr = bufnr })
        return true, 1
      end,
    } }
  end
end
local function settle()
  vim.wait(20, function() return false end)
end
local viewer = require('swara.viewer')
local preview = require('swara.preview')
]=]

--- Runs `body` in a child Neovim set up by `PRELUDE`, where the machine looks as `facts` says:
--- `mac` (default true), `skim` (Skim is installed, default true), `prefs` (what
--- `defaults read -app Skim <key>` prints for each key, a missing key being one never set),
--- `osascript` (its exit status, default 0) and `server` (`false` for no Swara client, or the `err`
--- and `result` the client answers every request with). `body` sees `viewer`, `preview`,
--- `settle()`, which lets queued callbacks run, and the recorders, and returns a table. That table
--- comes back decoded, with `runs` (each command given to `vim.system` but the preference reads),
--- `opened` (each path given to `vim.ui.open`), `messages` and `requests` added.
local function with_recorders(facts, body)
  facts = vim.tbl_extend('keep', facts, { mac = true, skim = true, prefs = {}, osascript = 0 })
  return h.child_json(table.concat({
    'local facts = ' .. vim.inspect(facts),
    PRELUDE,
    'local result = (function()',
    body,
    'end)()',
    'result.runs, result.opened, result.messages, result.requests = runs, opened, messages, requests',
    "io.stdout:write(vim.json.encode(result), '\\n')",
  }, '\n'))
end

--- A PDF that exists, so that its resolved path can be read, as `folder/name` in a new folder.
local function pdf(folder, name)
  return h.write(h.tmpdir() .. '/' .. folder .. '/' .. name, '%PDF-1.4\n')
end

--- The command that has Skim reload `file`, named by its resolved path.
local function revert(file)
  return {
    'osascript',
    '-e',
    'on run argv',
    '-e',
    'if application "Skim" is running then tell application "Skim" to revert (documents whose path is (item 1 of argv))',
    '-e',
    'end run',
    vim.uv.fs_realpath(file),
  }
end

--- A body that reports a build of each of `files` in turn, each with `success`.
local function builds(files, success)
  local lines = {}
  for _, file in ipairs(files) do
    table.insert(lines, ('viewer.built(0, { pdf = %q, success = %s })'):format(file, tostring(success ~= false)))
    table.insert(lines, 'settle()')
  end
  table.insert(lines, 'return {}')
  return table.concat(lines, '\n')
end

local function assert_runs(out, want)
  assert(vim.deep_equal(out.runs, want), 'ran ' .. vim.inspect(out.runs) .. ', expected ' .. vim.inspect(want))
end

function T.first_build_opens_in_background()
  local file = pdf('first', 'score.pdf')
  local out = with_recorders({}, builds({ file }))
  assert_runs(out, { { 'open', '-g', '-a', 'Skim', file } })
  assert(#out.opened == 0 and #out.messages == 0, vim.inspect(out))
end

function T.second_build_refreshes_per_spike()
  -- The file is reached through a link, so the resolved path differs from the given one. The test
  -- makes the link itself, because only macOS reaches its temporary folder through one.
  local link = h.tmpdir() .. '/second'
  assert(vim.uv.fs_symlink(vim.fs.dirname(pdf('second', 'score.pdf')), link))
  local file = link .. '/score.pdf'
  assert(vim.uv.fs_realpath(file) ~= file, 'the test needs a path that resolves to another')
  local cases = {
    { prefs = { SKAutoCheckFileUpdate = '1', SKAutoReloadFileUpdate = '1' }, reverts = false },
    { prefs = { SKAutoCheckFileUpdate = '1', SKAutoReloadFileUpdate = '0' }, reverts = true },
    { prefs = { SKAutoCheckFileUpdate = '0', SKAutoReloadFileUpdate = '1' }, reverts = true },
    { prefs = {}, reverts = true },
  }
  for _, case in ipairs(cases) do
    local out = with_recorders({ prefs = case.prefs }, builds({ file, file }))
    local want = { { 'open', '-g', '-a', 'Skim', file } }
    if case.reverts then
      table.insert(want, revert(file))
    end
    assert_runs(out, want)
    assert(#out.messages == 0, vim.inspect(out.messages))
  end
end

function T.revert_names_the_document_by_path()
  local a, b = pdf('a', 'one.pdf'), pdf('b', 'two.pdf')
  local out = with_recorders({}, builds({ a, b, a, b }))
  assert_runs(out, {
    { 'open', '-g', '-a', 'Skim', a },
    { 'open', '-g', '-a', 'Skim', b },
    revert(a),
    revert(b),
  })
  for _, argv in ipairs(out.runs) do
    for _, arg in ipairs(argv) do
      assert(not arg:find('document 1', 1, true), arg)
    end
  end
end

function T.failed_build_opens_nothing()
  local file = pdf('failed', 'score.pdf')
  local failed = with_recorders({}, builds({ file }, false))
  assert_runs(failed, {})
  assert(#failed.opened == 0, vim.inspect(failed.opened))
  assert(vim.deep_equal(failed.messages, { 'Swara: build failed, see the diagnostics' }), vim.inspect(failed.messages))
  -- Nor does a failure after a success refresh what Skim shows.
  local later = with_recorders(
    {},
    ('viewer.built(0, { pdf = %q, success = true })\nviewer.built(0, { pdf = %q, success = false })\nsettle()\nreturn {}'):format(
      file,
      file
    )
  )
  assert_runs(later, { { 'open', '-g', '-a', 'Skim', file } })
end

function T.path_with_spaces_and_bengali()
  local file = h.write(h.tmpdir() .. '/x/আমার গান/lesson 1.pdf', '%PDF-1.4\n')
  local out = with_recorders({}, builds({ file, file }))
  assert_runs(out, { { 'open', '-g', '-a', 'Skim', file }, revert(file) })
  assert(vim.endswith(out.runs[2][#out.runs[2]], '/x/আমার গান/lesson 1.pdf'), vim.inspect(out.runs[2]))
end

function T.no_skim_opens_with_the_default_app()
  -- macOS's default PDF viewer reloads a changed file by itself, so later builds do nothing.
  local file = pdf('noskim', 'score.pdf')
  local out = with_recorders({ skim = false }, builds({ file, file }))
  assert_runs(out, { { 'open', '-g', file } })
  assert(#out.opened == 0, vim.inspect(out.opened))
end

function T.elsewhere_uses_ui_open()
  local file = pdf('elsewhere', 'score.pdf')
  local out = with_recorders({ mac = false }, builds({ file, file }))
  assert_runs(out, {})
  assert(vim.deep_equal(out.opened, { file }), vim.inspect(out.opened))
end

function T.failed_reload_is_reported_once()
  local file = pdf('denied', 'score.pdf')
  local out = with_recorders({ osascript = 1 }, builds({ file, file, file }))
  assert(#out.runs == 3, vim.inspect(out.runs))
  assert(
    vim.deep_equal(out.messages, {
      'Swara: Skim did not reload the PDF: 83:98: execution error: Not authorized to send Apple events to Skim. (-1743)',
    }),
    vim.inspect(out.messages)
  )
end

function T.open_brings_skim_forward()
  local file = pdf('open', 'score.pdf')
  local out = with_recorders(
    {},
    ([[
    vim.b[0].swara_pdf = %q
    viewer.open(0)
    -- A build after that refreshes the PDF rather than opening it a second time.
    viewer.built(0, { pdf = %q, success = true })
    settle()
    return {}
  ]]):format(file, file)
  )
  assert_runs(out, { { 'open', '-a', 'Skim', file }, revert(file) })
end

function T.open_without_skim_uses_ui_open()
  local file = pdf('open', 'score.pdf')
  for _, facts in ipairs({ { skim = false }, { mac = false } }) do
    local out = with_recorders(facts, ('vim.b[0].swara_pdf = %q\nviewer.open(0)\nreturn {}'):format(file))
    assert_runs(out, {})
    assert(vim.deep_equal(out.opened, { file }), vim.inspect(out.opened))
  end
end

function T.open_before_a_build_says_so()
  local out = with_recorders({}, 'viewer.open(0)\nreturn {}')
  assert_runs(out, {})
  assert(vim.deep_equal(out.messages, { 'Swara: no PDF yet, build the file first' }), vim.inspect(out.messages))
end

--- A body that asks for a preview of a buffer named `score.ly`, after `setup` with `preview_dpi`
--- 300, and returns that buffer's URI plus whatever `extra` (Lua code) adds to the table `r`.
local function preview_body(before, extra)
  return ([[
    %s
    require('swara').setup({ preview_dpi = 300 })
    local buf = vim.api.nvim_get_current_buf()
    vim.api.nvim_buf_set_name(buf, vim.fn.tempname() .. '/score.ly')
    preview.show(buf)
    settle()
    local r = { uri = vim.uri_from_bufnr(buf) }
    %s
    return r
  ]]):format(before or '', extra or '')
end

local function assert_requested(out)
  assert(#out.requests == 1, vim.inspect(out.requests))
  assert(out.requests[1].method == 'workspace/executeCommand', out.requests[1].method)
  local want = { command = 'swara.preview', arguments = { out.uri, 300 } }
  assert(vim.deep_equal(out.requests[1].params, want), vim.inspect(out.requests[1].params))
end

local SNACKS = [[
  local attached
  package.loaded.snacks = {
    image = {
      supports = function()
        return supported
      end,
      buf = {
        attach = function(buf, opts)
          attached = { buf = buf, src = opts.src }
        end,
      },
    },
  }
]]

function T.preview_uses_snacks_when_present()
  local png = h.write(h.tmpdir() .. '/score.png', 'png')
  local out = with_recorders(
    { server = { result = { png = png } } },
    preview_body(
      'local supported = true\n' .. SNACKS,
      [[
      local win = attached and vim.fn.bufwinid(attached.buf) or -1
      r.src = attached and attached.src
      r.floating = win ~= -1 and vim.api.nvim_win_get_config(win).relative ~= ''
    ]]
    )
  )
  assert_requested(out)
  assert(out.src == png, 'snacks was given ' .. tostring(out.src))
  assert(out.floating, 'the image is not in a floating window')
  assert(#out.opened == 0 and #out.messages == 0, vim.inspect(out))
end

function T.preview_falls_back()
  local png = h.write(h.tmpdir() .. '/score.png', 'png')
  -- Without snacks.nvim, and with it in a terminal that cannot draw images.
  for _, before in ipairs({ '', 'local supported = false\n' .. SNACKS }) do
    local out = with_recorders(
      { server = { result = { png = png } } },
      preview_body(before, before ~= '' and 'r.attached = attached ~= nil' or '')
    )
    assert_requested(out)
    assert(vim.deep_equal(out.opened, { png }), vim.inspect(out.opened))
    assert(not out.attached, 'snacks drew an image it said it could not')
  end
end

function T.preview_reports_errors()
  local message = 'LilyPond made no image; build the file to see its errors'
  local out = with_recorders({ server = { err = { code = -32803, message = message } } }, preview_body())
  assert(vim.deep_equal(out.messages, { 'Swara: ' .. message }), vim.inspect(out.messages))
  assert(#out.opened == 0, vim.inspect(out.opened))
end

function T.preview_without_a_server_says_so()
  local out = with_recorders({ server = false }, preview_body())
  assert(#out.requests == 0, vim.inspect(out.requests))
  assert(vim.deep_equal(out.messages, { 'Swara: the server is not running for this file' }), vim.inspect(out.messages))
end

function T.preview_reaches_the_server()
  -- With the real server and no LilyPond, the answer is that LilyPond is missing, which the server
  -- gives only once the command's name and the file it names have passed its checks.
  local dir = h.tmpdir()
  local file = h.write(dir .. '/score.ly', '{ c4 }\n')
  local want = 'Swara: ' .. NOT_FOUND
  local out = h.child_json(([[
    local h = require('helpers')
    local messages = {}
    vim.notify = function(message)
      table.insert(messages, message)
    end
    require('swara').setup({ lilypond = %q })
    require('swara.lsp').setup({ path = %q, source = 'path', info = h.info() })
    vim.cmd.edit(%q)
    local buf = vim.api.nvim_get_current_buf()
    assert(h.wait_for(function()
      return vim.lsp.get_clients({ bufnr = buf, name = 'swara' })[1] ~= nil
    end, 5000), 'the server did not start')
    require('swara.preview').show(buf)
    h.wait_for(function()
      return vim.tbl_contains(messages, %q)
    end, 5000)
    io.stdout:write(vim.json.encode({ messages = messages }), '\n')
  ]]):format(dir .. '/no-lilypond', exe, file, want))
  assert(vim.tbl_contains(out.messages, want), vim.inspect(out.messages))
end

return T
