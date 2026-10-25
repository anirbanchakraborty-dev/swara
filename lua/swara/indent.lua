-- The indent of a line in a LilyPond buffer, from the captures in `queries/lilypond/indents.scm`:
-- one `shiftwidth()` for each block around the line, one fewer when the line starts by closing one.

local M = {}

--- The last column, 0-based, that holds a non-blank character on 1-based line `lnum`, or nil.
local function last_text(buf, lnum)
  local line = vim.api.nvim_buf_get_lines(buf, lnum - 1, lnum, false)[1] or ''
  local col = line:find('%S%s*$')
  return col and col - 1
end

--- Whether `node` goes on past the character at (`row`, `col`). A block whose closing bracket is
--- not typed yet counts, since the parser ends it with a missing bracket right after its last
--- token.
local function goes_past(node, row, col)
  local end_row, end_col = select(3, node:range())
  local last = node:child(node:child_count() - 1)
  return end_row > row or (end_row == row and end_col > col + 1) or (last ~= nil and last:missing())
end

--- The indent of 1-based line `lnum` of the current buffer, for `indentexpr`.
function M.get(lnum)
  local buf = vim.api.nvim_get_current_buf()
  local parser = vim.treesitter.get_parser(buf, 'lilypond', { error = false })
  local query = vim.treesitter.query.get('lilypond', 'indents')
  if not parser or not query then
    return -1
  end
  local root = parser:parse()[1]:root()
  local row = lnum - 1
  local line = vim.api.nvim_buf_get_lines(buf, row, lnum, false)[1] or ''
  local first = line:find('%S')

  -- A blank line, as one just opened, takes its blocks from the end of the line above: a block
  -- that goes on past that line's last character holds the new line too.
  local from_row, from_col = row, first and first - 1
  if not first then
    from_row = vim.fn.prevnonblank(lnum - 1) - 1
    if from_row < 0 then
      return 0
    end
    from_col = last_text(buf, from_row + 1)
  end

  local depth, closes = 0, false
  for id, node in query:iter_captures(root, buf, from_row, row + 1) do
    local name = query.captures[id]
    local start_row, start_col = node:start()
    if name == 'indent.begin' then
      local opens_before, holds
      if first then
        opens_before = start_row < row
        holds = vim.treesitter.is_in_node_range(node, row, from_col)
      else
        opens_before = start_row < from_row or (start_row == from_row and start_col <= from_col)
        holds = goes_past(node, from_row, from_col)
      end
      if opens_before and holds then
        depth = depth + 1
      end
    elseif first and (name == 'indent.branch' or name == 'indent.end') then
      closes = closes or (start_row == row and start_col == from_col)
    end
  end

  return math.max(depth - (closes and 1 or 0), 0) * vim.fn.shiftwidth()
end

return M
