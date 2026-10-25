-- Shared by the spec files. Everything a test creates on disk is removed when the run ends.

local M = {}

M.root = vim.fn.fnamemodify(debug.getinfo(1, 'S').source:sub(2), ':p:h:h')

local made = {}

--- Waits up to `timeout_ms` for `pred` to return true, and says whether it did.
function M.wait_for(pred, timeout_ms)
  return vim.wait(timeout_ms or 1000, pred, 10)
end

--- A new empty folder, removed by `cleanup`.
function M.tmpdir()
  local dir = vim.fn.tempname()
  vim.fn.mkdir(dir, 'p')
  table.insert(made, dir)
  return dir
end

--- Writes `text` to `path` byte for byte, making the folders above it.
function M.write(path, text)
  vim.fn.mkdir(vim.fn.fnamemodify(path, ':h'), 'p')
  local file = assert(io.open(path, 'wb'))
  file:write(text)
  file:close()
  return path
end

--- Reads `path` byte for byte, or returns nil when it does not exist.
function M.read(path)
  local file = io.open(path, 'rb')
  if not file then
    return nil
  end
  local text = file:read('*a')
  file:close()
  return text
end

local bin

--- Puts a shell script called `name` at the front of `PATH`. The runner restores `PATH` after each
--- test, so the script is seen only by the test that made it and the children it starts.
function M.fake_bin(name, script)
  if not bin or not vim.env.PATH:find(bin, 1, true) then
    bin = M.tmpdir() .. '/bin'
    vim.fn.mkdir(bin, 'p')
    vim.env.PATH = bin .. ':' .. vim.env.PATH
  end
  local path = M.write(bin .. '/' .. name, '#!/bin/sh\n' .. script .. '\n')
  vim.uv.fs_chmod(path, tonumber('755', 8))
  return path
end

--- Runs `lua` in a fresh `nvim --clean --headless -l`, with this repository on the runtime path
--- and these helpers on the module path, and returns what it printed and how it exited.
function M.child(lua, timeout_ms)
  local script = M.write(
    M.tmpdir() .. '/child.lua',
    ('vim.opt.runtimepath:prepend(%q)\npackage.path = %q .. package.path\n%s\n'):format(
      M.root,
      M.root .. '/tests/?.lua;',
      lua
    )
  )
  local result = vim
    .system({ vim.v.progpath, '--clean', '--headless', '-l', script }, { text = true })
    :wait(timeout_ms or 30000)
  return { code = result.code, stdout = result.stdout, stderr = result.stderr }
end

--- Runs `lua` through `child` and decodes the one JSON value it prints last.
function M.child_json(lua, timeout_ms)
  local result = M.child(lua, timeout_ms)
  assert(result.code == 0, ('child exited %d: %s%s'):format(result.code, result.stdout, result.stderr))
  local lines = vim.split(vim.trim(result.stdout .. result.stderr), '\n')
  return vim.json.decode(lines[#lines])
end

--- The decoded `swara info` of the debug build that `cargo build` leaves in `target/`, which gives
--- the tests the path of Swara's grammar library.
function M.info()
  local exe = M.root .. '/target/debug/swara'
  assert(vim.fn.executable(exe) == 1, 'no ' .. exe .. ': run `cargo build -p swara -p swara-parser`')
  local result = vim.system({ exe, 'info' }, { text = true }):wait(10000)
  assert(result.code == 0, ('`swara info` exited %d: %s'):format(result.code, result.stderr))
  return vim.json.decode(result.stdout)
end

--- Removes every folder `tmpdir` made.
function M.cleanup()
  for _, dir in ipairs(made) do
    vim.fn.delete(dir, 'rf')
  end
  made = {}
end

return M
