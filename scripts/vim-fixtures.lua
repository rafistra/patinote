-- Runs one Vim engine case in Neovim and writes the result (scripts/vim-fixtures.py).
--
-- Input ($VIM_CASE_IN): {"text": [lines], "cursor": [line (1-based), char col (0-based)],
-- "keys": [keys in Vim notation, one per item], "register": optional {"text": ...,
-- "linewise": bool}}.
-- Output ($VIM_CASE_OUT): lines, cursor (1-based line, 0-based char col), mode, and the
-- unnamed register.

local case = vim.json.decode(table.concat(vim.fn.readfile(vim.env.VIM_CASE_IN), "\n"))

-- Options that differ from `nvim --clean`, kept in sync with the engine (src/vim/mod.rs).
-- `j` in 'formatoptions' would strip comment leaders such as `#` or `-` on `J`, which is
-- wrong for Markdown.
vim.opt.formatoptions:remove("j")

-- Without undo levels, setting the text leaves nothing to undo.
local undolevels = vim.o.undolevels
vim.bo.undolevels = -1
vim.api.nvim_buf_set_lines(0, 0, -1, false, case.text)
vim.bo.undolevels = undolevels
vim.bo.modified = false
if case.register then
  vim.fn.setreg('"', case.register.text, case.register.linewise and "V" or "v")
end
vim.fn.setcursorcharpos(case.cursor[1], case.cursor[2] + 1)

local function dump()
  -- Keys still queued (a slow machine): look again a little later.
  if vim.fn.getchar(1) ~= 0 then
    vim.defer_fn(dump, 20)
    return
  end
  local pos = vim.fn.getcursorcharpos()
  local result = {
    lines = vim.api.nvim_buf_get_lines(0, 0, -1, false),
    cursor = { pos[2], pos[3] - 1 },
    mode = vim.api.nvim_get_mode().mode,
    register = {
      text = vim.fn.getreg('"'),
      linewise = vim.fn.getregtype('"') == "V",
    },
  }
  vim.fn.writefile({ vim.json.encode(result) }, vim.env.VIM_CASE_OUT)
  vim.cmd("qa!")
end

-- The keys go to Neovim's input queue as if typed (`nvim_input`). The dump runs
-- once Neovim is idle again, in whatever mode the keys ended in (insert included).
-- Deferred callbacks do not run while a command still waits for a key, so an
-- unfinished command shows up as a timeout instead of a wrong result.
vim.api.nvim_input(table.concat(case.keys))
vim.defer_fn(dump, 50)
