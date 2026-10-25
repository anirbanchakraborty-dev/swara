-- Opens the PDF a build leaves, in Skim when it is installed and without taking focus from the
-- editor, and keeps it current after each later build. The refresh route is the one chosen in
-- `docs/spikes/skim.md`.

local M = {}

-- The PDFs opened in this session, by the path the server gave.
local shown = {}

-- Whether a reload that failed has been reported, which happens once a session.
local warned = false

-- The two preferences that make Skim reload a changed PDF by itself. Skim ships with both off.
M.WATCH = { 'SKAutoCheckFileUpdate', 'SKAutoReloadFileUpdate' }

-- Has Skim reload the document open at the path given as the script's argument. It names the
-- document by its path because `document 1` is whichever PDF is in front, and the `running` test
-- keeps a Skim the user has quit from being launched again at each build.
local REVERT =
  'if application "Skim" is running then tell application "Skim" to revert (documents whose path is (item 1 of argv))'

local function on_mac()
  return vim.fn.has('mac') == 1
end

--- Whether Skim is installed in either folder macOS keeps applications in.
function M.has_skim()
  for _, dir in ipairs({ '/Applications', vim.fs.normalize('~/Applications') }) do
    if vim.fn.isdirectory(dir .. '/Skim.app') == 1 then
      return true
    end
  end
  return false
end

--- Calls `done(true)` when both watch preferences read 1, and `done(false)` otherwise. An unset
--- preference makes `defaults` exit 1, and means Skim's shipped value, which is off.
local function skim_watches(done)
  local on, left = {}, #M.WATCH
  for i, key in ipairs(M.WATCH) do
    vim.system({ 'defaults', 'read', '-app', 'Skim', key }, { text = true }, function(result)
      on[i] = result.code == 0 and vim.trim(result.stdout) == '1'
      left = left - 1
      if left == 0 then
        vim.schedule(function()
          done(on[1] and on[2])
        end)
      end
    end)
  end
end

--- Has Skim reload `pdf`, named by its resolved path because Skim holds resolved paths.
local function revert(pdf)
  local path = vim.uv.fs_realpath(pdf) or pdf
  vim.system(
    { 'osascript', '-e', 'on run argv', '-e', REVERT, '-e', 'end run', path },
    { text = true },
    function(result)
      if result.code ~= 0 and not warned then
        warned = true
        vim.schedule(function()
          vim.notify('Swara: Skim did not reload the PDF: ' .. vim.trim(result.stderr), vim.log.levels.WARN)
        end)
      end
    end
  )
end

local function ui_open(path)
  local _, err = vim.ui.open(path)
  if err then
    vim.notify('Swara: ' .. err, vim.log.levels.WARN)
  end
end

--- Called when a build of `buf` ends, with the `swara/buildFinished` parameters. The first
--- successful build of a PDF opens it in the background. A later one has Skim reload it, unless
--- Skim reloads changed files by itself. Without Skim the PDF viewer is left to notice the change.
function M.built(_, params)
  if not params.success then
    vim.notify('Swara: build failed, see the diagnostics', vim.log.levels.WARN)
    return
  end
  local pdf = params.pdf
  if type(pdf) ~= 'string' then
    return
  end
  if not shown[pdf] then
    shown[pdf] = true
    if not on_mac() then
      ui_open(pdf)
    elseif M.has_skim() then
      vim.system({ 'open', '-g', '-a', 'Skim', pdf })
    else
      vim.system({ 'open', '-g', pdf })
    end
  elseif on_mac() and M.has_skim() then
    skim_watches(function(watching)
      if not watching then
        revert(pdf)
      end
    end)
  end
end

--- Opens the PDF of `buf`'s last build and brings its viewer forward.
function M.open(buf)
  local pdf = vim.b[buf].swara_pdf
  if type(pdf) ~= 'string' then
    vim.notify('Swara: no PDF yet, build the file first', vim.log.levels.INFO)
    return
  end
  shown[pdf] = true
  if on_mac() and M.has_skim() then
    vim.system({ 'open', '-a', 'Skim', pdf })
  else
    ui_open(pdf)
  end
end

return M
