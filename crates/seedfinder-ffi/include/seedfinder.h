#ifndef SEEDFINDER_H
#define SEEDFINDER_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// All functions are thread-safe. Packets use the same wire formats as JNI:
// Search requests use SSF8 and results use SSR1. SSF8 globals are:
// magic[4], max_depth:u8, flags:u8, challenges:u16 little-endian,
// requirement_count:u16 big-endian; tier mode 3 means at most. Each
// requirement carries, in order: kind:u8, item id:utf8_u16, tier mode+value,
// upgrade mode+value, an effect predicate (mode:u8 0 = any; 1 = one-of,
// followed by count:u8 and that many utf8_u16 wire names of the same family),
// source:u8 (0 = any, else wire id + 1), identity_group:u8 (0 = none),
// max_depth:u8 (0 = none), alternative_group:u8 (0 = none; equal non-zero
// groups are alternatives satisfied by any one member), combined-upgrade
// sum_group:u8 and sum_total:u8 (0/0 = none; members of one group must be
// matched by distinct items whose upgrades total at least sum_total), and
// flags:u8 where bit 0 requires an uncursed item.
// Scout requests are SSQ2 magic[4], challenges:u16 little-endian, then the
// UTF-8 seed code in all remaining bytes. Legacy raw UTF-8 seed codes use mask 0.
// Scout responses remain SSC1.
int64_t seedfinder_start_search(const uint8_t *request, size_t request_len); // >0 handle, 0 on invalid request or spawn failure
int32_t seedfinder_poll(int64_t handle, uint32_t max_results, uint8_t **out_packet, size_t *out_len);
int32_t seedfinder_status(int64_t handle, int64_t out_status[5]); // [state, scanned, total, errorCode, probabilityBits]
void    seedfinder_cancel(int64_t handle);
void    seedfinder_close(int64_t handle);
int32_t seedfinder_scout(const uint8_t *request, size_t request_len, uint8_t **out_packet, size_t *out_len);
void    seedfinder_buffer_free(uint8_t *ptr, size_t len);

#ifdef __cplusplus
}
#endif

#endif
