-- Starts Swara's language server through Neovim's own LSP client, and keeps what each build
-- reports on the buffer it built.

local config = require('swara.config')

local M = {}

local started = false

--- The loaded buffer holding the file `uri` names, or nil. It never makes a buffer, as
--- `vim.uri_to_bufnr` would for a file that is not open.
local function buffer_of(uri)
  local name = vim.uri_to_fname(uri)
  for _, buf in ipairs(vim.api.nvim_list_bufs()) do
    if vim.api.nvim_buf_is_loaded(buf) and vim.api.nvim_buf_get_name(buf) == name then
      return buf
    end
  end
end

--- The handler for `swara/buildFinished`, which the server sends when a build ends. It stores the
--- PDF and MIDI paths on the buffer that was built, then hands the result to the viewer.
function M.on_build(err, params, _)
  if err or type(params) ~= 'table' or type(params.uri) ~= 'string' then
    return
  end
  local buf = buffer_of(params.uri)
  if not buf then
    return
  end
  vim.b[buf].swara_pdf = params.pdf
  vim.b[buf].swara_midi = params.midi
  require('swara.viewer').built(buf, params)
end

--- Registers the server `bin.path` as `swara` and enables it for LilyPond buffers, once per
--- session. A file outside any repository gets a server with no root.
function M.setup(bin)
  if started then
    return
  end
  started = true
  local cfg = config.get()
  vim.lsp.config('swara', {
    cmd = { bin.path, 'lsp' },
    filetypes = { 'lilypond' },
    root_markers = { '.git' },
    init_options = { autoCheck = cfg.auto_check, lilypond = cfg.lilypond, includePaths = cfg.include_paths },
    flags = { debounce_text_changes = cfg.debounce_ms },
    handlers = { ['swara/buildFinished'] = M.on_build },
  })
  vim.lsp.enable('swara')
end

return M
