# LLDB Recipes

## Start App

```bash
lldb target/debug/arcade-app
run

bt
thread backtrace all

register read
register read x30
register read lr
register read pc
register read sp

memory read -fx -s8 -c 4 ADDRESS
memory read --format x --size 8 --count 8 ADDRESS

breakpoint set --name FUNCTION_NAME
breakpoint set --file FILE --line LINE


