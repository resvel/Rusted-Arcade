/* * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * *
 *   Mupen64plus - rdp_core.c                                              *
 *   Mupen64Plus homepage: http://code.google.com/p/mupen64plus/           *
 *   Copyright (C) 2014 Bobby Smiles                                       *
 *                                                                         *
 *   This program is free software; you can redistribute it and/or modify  *
 *   it under the terms of the GNU General Public License as published by  *
 *   the Free Software Foundation; either version 2 of the License, or     *
 *   (at your option) any later version.                                   *
 *                                                                         *
 *   This program is distributed in the hope that it will be useful,       *
 *   but WITHOUT ANY WARRANTY; without even the implied warranty of        *
 *   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the         *
 *   GNU General Public License for more details.                          *
 *                                                                         *
 *   You should have received a copy of the GNU General Public License     *
 *   along with this program; if not, write to the                         *
 *   Free Software Foundation, Inc.,                                       *
 *   51 Franklin Street, Fifth Floor, Boston, MA 02110-1301, USA.          *
 * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * * */

#include "rdp_core.h"

#include "../main/device.h"
#include "../main/main.h"
#include "../memory/memory.h"
#include "../plugin/plugin.h"
#include "../r4300/r4300_core.h"
#include "../rsp/rsp_core.h"

#include <string.h>

static void ensure_rdp_links(struct rdp_core* dp)
{
   if (dp == NULL)
      return;

   if (dp->r4300 == NULL)
      dp->r4300 = &g_dev.r4300;
   if (dp->sp == NULL)
      dp->sp = &g_dev.sp;
   if (dp->ri == NULL)
      dp->ri = &g_dev.ri;

   if (dp->sp != NULL)
   {
      if (dp->sp->r4300 == NULL)
         dp->sp->r4300 = &g_dev.r4300;
      if (dp->sp->dp == NULL)
         dp->sp->dp = &g_dev.dp;
      if (dp->sp->ri == NULL)
         dp->sp->ri = &g_dev.ri;
   }
}

static int update_dpc_status(struct rdp_core* dp, uint32_t w)
{
   ensure_rdp_links(dp);
   if (dp == NULL)
      return 0;

   /* see do_SP_Task for more info */
   int do_sp_task_on_unfreeze = 0;

   /* clear / set xbus_dmem_dma */
   if (w & DPC_STATUS_CLR_XBUS_DMEM_DMA) dp->dpc_regs[DPC_STATUS_REG] &= ~DPC_STATUS_XBUS_DMEM_DMA;
   if (w & DPC_STATUS_SET_XBUS_DMEM_DMA) dp->dpc_regs[DPC_STATUS_REG] |= DPC_STATUS_XBUS_DMEM_DMA;

   /* clear / set freeze */
   if (w & DPC_STATUS_CLR_FREEZE)
   {
      dp->dpc_regs[DPC_STATUS_REG] &= ~DPC_STATUS_FREEZE;

      if (dp->sp != NULL && !(dp->sp->regs[SP_STATUS_REG] & (SP_STATUS_HALT | SP_STATUS_BROKE)))
         do_sp_task_on_unfreeze = 1;
   }

   if (w & DPC_STATUS_SET_FREEZE) dp->dpc_regs[DPC_STATUS_REG] |= DPC_STATUS_FREEZE;

   /* clear / set flush */
   if (w & DPC_STATUS_CLR_FLUSH) dp->dpc_regs[DPC_STATUS_REG] &= ~DPC_STATUS_FLUSH;
   if (w & DPC_STATUS_SET_FLUSH) dp->dpc_regs[DPC_STATUS_REG] |= DPC_STATUS_FLUSH;

   return do_sp_task_on_unfreeze;
}


void init_rdp(struct rdp_core* dp,
                 struct r4300_core* r4300,
                 struct rsp_core* sp,
                 struct ri_controller *ri)
{
    dp->r4300 = r4300;
    dp->sp    = sp;
    dp->ri    = ri;
}

void poweron_rdp(struct rdp_core* dp)
{
    memset(dp->dpc_regs, 0, DPC_REGS_COUNT*sizeof(uint32_t));
    memset(dp->dps_regs, 0, DPS_REGS_COUNT*sizeof(uint32_t));

    poweron_fb(&dp->fb);
}


int read_dpc_regs(void* opaque, uint32_t address, uint32_t* value)
{
    struct rdp_core* dp = (struct rdp_core*)opaque;
    uint32_t reg        = DPC_REG(address);
    ensure_rdp_links(dp);

    if (dp == NULL)
    {
        if (value != NULL)
            *value = 0;
        return 0;
    }

    *value              = (reg < DPC_REGS_COUNT) ? dp->dpc_regs[reg] : 0u;

    return 0;
}

int write_dpc_regs(void* opaque, uint32_t address, uint32_t value, uint32_t mask)
{
   struct rdp_core* dp = (struct rdp_core*)opaque;
   uint32_t reg        = DPC_REG(address);
   ensure_rdp_links(dp);
   if (dp == NULL)
      return 0;

   switch(reg)
   {
      case DPC_STATUS_REG:
         if (update_dpc_status(dp, value & mask) != 0)
            do_SP_Task(dp->sp);
      case DPC_CURRENT_REG:
      case DPC_CLOCK_REG:
      case DPC_BUFBUSY_REG:
      case DPC_PIPEBUSY_REG:
      case DPC_TMEM_REG:
         return 0;
   }

   if (reg < DPC_REGS_COUNT)
       dp->dpc_regs[reg] = MASKED_WRITE(&dp->dpc_regs[reg], value, mask);

   switch(reg)
   {
      case DPC_START_REG:
         dp->dpc_regs[DPC_CURRENT_REG] = dp->dpc_regs[DPC_START_REG];
         break;
      case DPC_END_REG:
         gfx.processRDPList();
         signal_rcp_interrupt(dp->r4300, MI_INTR_DP);
         break;
   }

   return 0;
}


int read_dps_regs(void* opaque, uint32_t address, uint32_t* value)
{
    struct rdp_core* dp = (struct rdp_core*)opaque;
    uint32_t reg        = DPS_REG(address);
    ensure_rdp_links(dp);

    if (dp == NULL)
    {
        if (value != NULL)
            *value = 0;
        return 0;
    }

    *value = (reg < DPS_REGS_COUNT) ? dp->dps_regs[reg] : 0u;

    return 0;
}

int write_dps_regs(void* opaque, uint32_t address, uint32_t value, uint32_t mask)
{
    struct rdp_core* dp = (struct rdp_core*)opaque;
    uint32_t reg        = DPS_REG(address);
    ensure_rdp_links(dp);

    if (dp == NULL)
        return 0;

    if (reg < DPS_REGS_COUNT)
        dp->dps_regs[reg] = MASKED_WRITE(&dp->dps_regs[reg], value, mask);

    return 0;
}

void rdp_interrupt_event(struct rdp_core* dp)
{
   ensure_rdp_links(dp);
   if (dp == NULL || dp->r4300 == NULL)
      return;

   raise_rcp_interrupt(dp->r4300, MI_INTR_DP);
}
