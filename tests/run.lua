-- Runs every tests/*_spec.lua file, or only the files whose names contain one of the arguments:
--
--   nvim --clean --headless -l tests/run.lua [filter ...]
--
-- Each spec file returns a table of named functions. Each function is called with a fresh scratch
-- buffer, in name order, and the run exits 1 when any of them fails.

local root = vim.fn.fnamemodify(debug.getinfo(1, 'S').source:sub(2), ':p:h:h')
vim.opt.runtimepath:prepend(root)
package.path = root .. '/tests/?.lua;' .. package.path

local helpers = require('helpers')

local function wanted(file)
  if #arg == 0 then
    return true
  end
  for _, filter in ipairs(arg) do
    if file:find(filter, 1, true) then
      return true
    end
  end
  return false
end

local function say(line)
  io.stdout:write(line, '\n')
  io.stdout:flush()
end

local passed, failed = 0, 0
for _, file in ipairs(vim.fn.glob(root .. '/tests/*_spec.lua', false, true)) do
  local spec = vim.fn.fnamemodify(file, ':t:r')
  if wanted(spec) then
    local tests = dofile(file)
    local names = vim.tbl_keys(tests)
    table.sort(names)
    for _, name in ipairs(names) do
      local path = vim.env.PATH
      local buf = vim.api.nvim_create_buf(true, true)
      vim.api.nvim_set_current_buf(buf)
      local ok, err = xpcall(tests[name], debug.traceback, buf)
      vim.env.PATH = path
      if vim.api.nvim_buf_is_valid(buf) then
        vim.api.nvim_buf_delete(buf, { force = true })
      end
      if ok then
        passed = passed + 1
        say('ok ' .. spec .. '.' .. name)
      else
        failed = failed + 1
        say('FAIL ' .. spec .. '.' .. name .. ': ' .. tostring(err))
      end
    end
  end
end

helpers.cleanup()
say(('%d passed, %d failed'):format(passed, failed))
-- `cquit` rather than `os.exit`, so that Neovim removes its own temporary folder on the way out.
vim.cmd.cquit(failed > 0 and 1 or 0)
