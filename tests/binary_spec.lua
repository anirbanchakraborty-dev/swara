local h = require('helpers')

local T = {}

local INFO = '{"version":"0.1.0","grammar":null,"grammar_symbol":"lilypond_swara",'
  .. '"grammar_rev":"test","abi":15,"lilypond":null,"cache":null}'
local PRINT_INFO = "printf '%s\\n' '" .. INFO .. "'"
-- Holds `curl` and `tar` on macOS and Linux, and no `swara`.
local SYSTEM_PATH = '/usr/bin:/bin'

local function triple()
  return assert(require('swara.binary').triple(), 'no release triple for this machine')
end

local function executable(path, script)
  h.write(path, '#!/bin/sh\n' .. script .. '\n')
  vim.uv.fs_chmod(path, tonumber('755', 8))
  return path
end

--- A release folder as GitHub would serve it, reached through a `file://` URL. `opts.sum` replaces
--- the archive's real checksum in `SHA256SUMS`.
local function release(opts)
  opts = opts or {}
  local root = h.tmpdir()
  local name = 'swara-0.1.0-' .. triple()
  executable(root .. '/stage/' .. name .. '/bin/swara', PRINT_INFO)
  local folder = root .. '/v0.1.0'
  vim.fn.mkdir(folder, 'p')
  local archive = folder .. '/' .. name .. '.tar.gz'
  local tar = vim.system({ 'tar', '-czf', archive, '-C', root .. '/stage', name }):wait()
  assert(tar.code == 0, tar.stderr)
  local sum = opts.sum or vim.fn.sha256(vim.fn.readblob(archive))
  h.write(
    folder .. '/SHA256SUMS',
    ('%s  swara-0.1.0-other-platform.tar.gz\n%s  %s.tar.gz\n'):format(string.rep('0', 64), sum, name)
  )
  return 'file://' .. root
end

--- Resolves `swara` in a fresh Neovim whose data folder is `env.data`, once or (with `env.twice`)
--- twice, and returns each result with every notification it gave.
local function resolve(env)
  return h.child_json(([[
    vim.env.XDG_DATA_HOME = %q
    vim.env.PATH = %q
    local notes = {}
    vim.notify = function(message, level) table.insert(notes, { message = message, level = level }) end
    require('swara').setup({ release_base_url = %q })
    local binary = require('swara.binary')
    local function once()
      local done, got, message = false, nil, nil
      binary.resolve(function(result, why) done, got, message = true, result, why end)
      assert(vim.wait(20000, function() return done end, 10), 'resolve never called back')
      return { got = got, message = message, notes = #notes }
    end
    local first = once()
    local second = %s and once() or nil
    print(vim.json.encode({ first = first, second = second, notes = notes }))
  ]]):format(env.data, env.path or SYSTEM_PATH, env.base, tostring(env.twice == true)))
end

local function same_file(a, b)
  return vim.uv.fs_realpath(a) ~= nil and vim.uv.fs_realpath(a) == vim.uv.fs_realpath(b)
end

local function exists(path)
  return vim.uv.fs_stat(path) ~= nil
end

function T.prefers_path()
  local exe = h.fake_bin('swara', PRINT_INFO)
  local got = resolve({
    data = h.tmpdir(),
    path = vim.fn.fnamemodify(exe, ':h') .. ':' .. SYSTEM_PATH,
    base = 'file:///nowhere',
  })
  local found = assert(got.first.got, vim.inspect(got))
  assert(found.source == 'path', found.source)
  assert(same_file(found.path, exe), found.path)
  assert(found.info.version == '0.1.0' and found.info.grammar_symbol == 'lilypond_swara', vim.inspect(found.info))
  assert(#got.notes == 0, vim.inspect(got.notes))
end

function T.downloads_and_verifies()
  local data = h.tmpdir()
  local got = resolve({ data = data, base = release(), twice = true })
  local found = assert(got.first.got, vim.inspect(got))
  local want = data .. '/nvim/swara/0.1.0/bin/swara'
  assert(found.source == 'download', found.source)
  assert(same_file(found.path, want), found.path)
  assert(vim.fn.executable(want) == 1, want .. ' is not executable')
  assert(found.info.version == '0.1.0', vim.inspect(found.info))
  assert(not exists(data .. '/nvim/swara/0.1.0.partial'), 'the .partial folder was left behind')
  assert(#got.notes == 2, vim.inspect(got.notes))
  assert(got.notes[1].message == 'Swara: downloading swara 0.1.0 for ' .. triple(), got.notes[1].message)
  assert(got.notes[2].message == 'Swara: ready', got.notes[2].message)
  local again = assert(got.second.got, vim.inspect(got.second))
  assert(again.source == 'download' and same_file(again.path, want), vim.inspect(again))
  assert(got.second.notes == 2, 'the second lookup downloaded again')
end

function T.uses_earlier_download()
  local data = h.tmpdir()
  assert(resolve({ data = data, base = release() }).first.got, 'the first download failed')
  local got = resolve({ data = data, base = 'file:///nowhere' })
  local found = assert(got.first.got, vim.inspect(got))
  assert(found.source == 'download', found.source)
  assert(same_file(found.path, data .. '/nvim/swara/0.1.0/bin/swara'), found.path)
  assert(#got.notes == 0, vim.inspect(got.notes))
end

function T.bad_checksum_refused()
  local data = h.tmpdir()
  local got = resolve({ data = data, base = release({ sum = string.rep('1', 64) }) })
  assert(got.first.got == nil, vim.inspect(got.first.got))
  assert(got.first.message:find('checksum', 1, true), got.first.message)
  assert(not exists(data .. '/nvim/swara/0.1.0'), 'the refused archive was installed')
  assert(not exists(data .. '/nvim/swara/0.1.0.partial'), 'the .partial folder was left behind')
  local last = got.notes[#got.notes].message
  assert(last:find('Swara: download failed (', 1, true) and last:find('checksum', 1, true), last)
end

function T.offline_message_once()
  local data = h.tmpdir()
  local got = resolve({ data = data, base = 'file://' .. h.tmpdir() .. '/missing', twice = true })
  assert(got.first.got == nil and got.second.got == nil, vim.inspect(got))
  local failures = vim.tbl_filter(function(note)
    return note.message:find('download failed', 1, true) ~= nil
  end, got.notes)
  assert(#failures == 1, vim.inspect(got.notes))
  local message = failures[1].message
  assert(message:find('LilyPond files stay plain text until the next start', 1, true), message)
  assert(got.second.notes == got.first.notes, 'the second buffer gave a message: ' .. vim.inspect(got.notes))
  assert(got.second.message, 'the second lookup gave no reason')
  assert(not exists(data .. '/nvim/swara/0.1.0.partial'), 'the .partial folder was left behind')
end

function T.partial_never_used()
  local data = h.tmpdir()
  local marker = h.tmpdir() .. '/ran'
  local partial = data .. '/nvim/swara/0.1.0.partial'
  executable(partial .. '/bin/swara', 'touch ' .. marker .. '\n' .. PRINT_INFO)
  h.write(partial .. '/bin/left-over', 'from an earlier attempt\n')
  local got = resolve({ data = data, base = release() })
  local found = assert(got.first.got, vim.inspect(got))
  assert(found.source == 'download', found.source)
  assert(not exists(marker), 'the binary in the leftover .partial folder was run')
  assert(not exists(partial), 'the .partial folder was left behind')
  assert(not exists(data .. '/nvim/swara/0.1.0/bin/left-over'), 'the leftover .partial folder was reused')
end

return T
