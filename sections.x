/* Sample sections.x file */
/* Ensure at least 1KB of RAM is left for heap space. */
.heap_guard :
{
  . = ALIGN(4);
  . = . + 1K;
  . = ALIGN(4);
} > RAM
