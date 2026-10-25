-- What happens to the PDF a build leaves. For now nothing more than `swara.lsp` already does,
-- which is to store its path on the buffer.

local M = {}

--- Called when a build of `buf` ends, with the `swara/buildFinished` parameters.
function M.built(_, _) end

return M
