#ifndef SEEDFINDER_H
#define SEEDFINDER_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// All functions are thread-safe. Packets use the same wire formats as JNI:
// Search requests use SSF7 and results use SSR1. SSF7 globals are:
// magic[4], max_depth:u8, flags:u8, challenges:u16 little-endian,
// requirement_count:u16 big-endian; tier mode 3 means at most. Each requirement
// appends flags:u8 where bit 0 requires an uncursed item.
// Scout requests are SSQ2 magic[4], challenges:u16 little-endian, then the
// UTF-8 seed code in all remaining bytes. Legacy raw UTF-8 seed codes use mask 0.
// Scout responses remain SSC1.
int64_t seedfinder_start_search(const uint8_t *request, size_t request_len); // >0 handle, 0 on invalid request or spawn failure
// Starts a search that scans only the scan_len seeds beginning at resume_from,
// wrapping at the end of the seed space. Pass the values reported by
// seedfinder_resume_hint on the stopped session being refined.
int64_t seedfinder_start_resumed_search(const uint8_t *request, size_t request_len, uint64_t resume_from, uint64_t scan_len);
int32_t seedfinder_poll(int64_t handle, uint32_t max_results, uint8_t **out_packet, size_t *out_len);
int32_t seedfinder_status(int64_t handle, int64_t out_status[5]); // [state, scanned, total, errorCode, probabilityBits]
// Writes [resume_position, remaining]: where and how much a follow-up search
// must scan to finish this session's coverage. Exact once the session stopped.
int32_t seedfinder_resume_hint(int64_t handle, int64_t out_hint[2]);
void    seedfinder_cancel(int64_t handle);
void    seedfinder_close(int64_t handle);
int32_t seedfinder_scout(const uint8_t *request, size_t request_len, uint8_t **out_packet, size_t *out_len);
// Re-verifies seeds_len numeric seed values against the SSF7 query in request
// and returns the surviving seeds as an SSR1 packet in input order.
int32_t seedfinder_filter_seeds(const uint8_t *request, size_t request_len, const uint64_t *seeds, size_t seeds_len, uint8_t **out_packet, size_t *out_len);
void    seedfinder_buffer_free(uint8_t *ptr, size_t len);

#ifdef __cplusplus
}
#endif

#endif
