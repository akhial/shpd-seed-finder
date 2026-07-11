#ifndef SEEDFINDER_H
#define SEEDFINDER_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// All functions are thread-safe. Packets use the same wire formats as JNI:
// request SSF2/SSF3, results SSR1, scout SSC1 (documented in the Kotlin adapter).
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
