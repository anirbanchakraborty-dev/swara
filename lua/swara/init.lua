-- Swara for Neovim. `setup` is optional: `plugin/swara.lua` attaches every LilyPond buffer with
-- the defaults when it was never called.

local M = {}

--- Sets Swara's options. See `swara.config` for the names and their defaults.
function M.setup(opts)
  require('swara.config').setup(opts)
end

--- Called for each buffer whose filetype becomes `lilypond`. Once the `swara` program is found,
--- which may mean a download, the buffer gets colours, the server and tidy on save. A buffer
--- opened while the download runs waits for it with the others.
function M.attach(buf)
  require('swara.binary').resolve(function(bin)
    -- A failed search has already said why, once.
    if not bin or not vim.api.nvim_buf_is_valid(buf) or vim.bo[buf].filetype ~= 'lilypond' then
      return
    end
    require('swara.treesitter').attach(buf, bin.info)
    require('swara.lsp').setup(bin)
    require('swara.format').attach(buf)
  end)
end

return M
