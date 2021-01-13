MEMORY
{
  FLASH (rx)      : ORIGIN = 0x8000000, LENGTH = 1024K
  /* reserved for config data */
  CONFIG (rx)     : ORIGIN = 0x8100000, LENGTH = 16K
  RAM (xrw)       : ORIGIN = 0x20000000, LENGTH = 112K - 4
  /* reserved for DFU trigger message */
  DFU_MSG (wrx)   : ORIGIN = 0x2001BFFC, LENGTH = 4
  RAM2 (xrw)      : ORIGIN = 0x2001C000, LENGTH = 16K
  RAM3 (xrw)      : ORIGIN = 0x20020000, LENGTH = 64K
  CCMRAM (rw)     : ORIGIN = 0x10000000, LENGTH = 64K
}

_flash_start = ORIGIN(FLASH);
_config_start = ORIGIN(CONFIG);
_dfu_msg = ORIGIN(DFU_MSG);
_stack_start = ORIGIN(CCMRAM) + LENGTH(CCMRAM);
