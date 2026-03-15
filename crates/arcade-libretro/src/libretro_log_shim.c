#include <stdarg.h>
#include <stdio.h>

void arcade_libretro_log_printf(int level, const char *fmt, ...)
{
    (void)level;

    if (fmt == NULL) {
        return;
    }

    va_list args;
    va_start(args, fmt);
    vfprintf(stderr, fmt, args);
    va_end(args);
}
