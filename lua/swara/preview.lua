-- Shows the score as an image inside the editor. Swara's server engraves it to a PNG, which
-- snacks.nvim draws in a floating window when it is loaded and the terminal can draw images, and
-- which the system's image viewer opens otherwise.

local config = require('swara.config')

local M = {}

-- The floating window of the last preview, closed when the next one opens.
local win

--- The snacks.nvim image module when it is loaded and can draw `png` in this terminal. The calls
--- used are `Snacks.image.supports(file)`, which checks the file's format and the terminal, and
--- `Snacks.image.buf.attach(buf, { src = file })`, which turns `buf` into a view of `file`, as read
--- in snacks.nvim v2.31.0 on 2026-10-25.
local function snacks_image(png)
  local snacks = package.loaded.snacks
  if type(snacks) ~= 'table' then
    return nil
  end
  local loaded, image = pcall(function()
    return snacks.image
  end)
  if not loaded or type(image) ~= 'table' or type(image.supports) ~= 'function' then
    return nil
  end
  local asked, supported = pcall(image.supports, png)
  if asked and supported then
    return image
  end
end

local function ui_open(path)
  local _, err = vim.ui.open(path)
  if err then
    vim.notify('Swara: ' .. err, vim.log.levels.WARN)
  end
end

--- Draws `png` with snacks.nvim in a floating window over most of the editor, closed with `q`.
local function float(image, png)
  if win and vim.api.nvim_win_is_valid(win) then
    vim.api.nvim_win_close(win, true)
  end
  local buf = vim.api.nvim_create_buf(false, true)
  vim.bo[buf].bufhidden = 'wipe'
  local width = math.floor(vim.o.columns * 0.8)
  local height = math.floor((vim.o.lines - vim.o.cmdheight) * 0.8)
  win = vim.api.nvim_open_win(buf, true, {
    relative = 'editor',
    width = width,
    height = height,
    row = math.floor((vim.o.lines - height) / 2) - 1,
    col = math.floor((vim.o.columns - width) / 2),
    style = 'minimal',
    border = 'rounded',
    title = ' Swara preview ',
    title_pos = 'center',
  })
  vim.keymap.set('n', 'q', '<Cmd>close<CR>', { buffer = buf, desc = 'Close the Swara preview' })
  if not pcall(image.buf.attach, buf, { src = png }) then
    vim.api.nvim_win_close(win, true)
    ui_open(png)
  end
end

--- Asks Swara's server to engrave `buf` as an image at `preview_dpi`, and shows the image.
function M.show(buf)
  local client = vim.lsp.get_clients({ bufnr = buf, name = 'swara' })[1]
  if not client then
    vim.notify('Swara: the server is not running for this file', vim.log.levels.WARN)
    return
  end
  local params = {
    command = 'swara.preview',
    arguments = { vim.uri_from_bufnr(buf), config.get().preview_dpi },
  }
  client:request('workspace/executeCommand', params, function(err, result)
    if err then
      vim.notify('Swara: ' .. err.message, vim.log.levels.WARN)
      return
    end
    local png = type(result) == 'table' and result.png
    if type(png) ~= 'string' then
      return
    end
    local image = snacks_image(png)
    if image then
      float(image, png)
    else
      ui_open(png)
    end
  end, buf)
end

return M
