/* fb_gpu.h — Phase 2 GPU raster + present path (Vulkan swapchain, live no-readback).
 *
 * Scaffold/contract:
 * - Live path renders to an acquired swapchain image and presents directly
 *   (no host readback/copy on present). --first-frame/dump and
 *   SH2_FB_GPU_PROVE=compare use a separate on-demand readback path.
 * - Refuse > guess: init/capability/surface/swapchain/SPIR-V/dispatch failure
 *   => unavailable with a clear reason and CPU fallback; never claim success.
 * - Project overrides win: SH2_FB_CPU_ONLY=1 or SH2_FB_GPU=0 disables GPU.
 * - Nvidia preferred when enumerating; mixed ICDs tolerated; honors
 *   VK_ICD_FILENAMES/VK_DRIVER_FILES for diagnosis. Lavapipe correctness-only.
 */
#ifndef FB_GPU_H
#define FB_GPU_H

#include <stdint.h>

/* Probe/init the GPU path for a window-backed surface of w*h. Returns 1 if the
 * live GPU swapchain path is ready, 0 on refusal/unavailable (reason set). Never
 * guesses: any uncertainty => 0. Safe to call when no display/Vulkan exists. */
int fb_gpu_try_init(int w, int h);

/* 1 when the live GPU swapchain path is active and will present without readback. */
int fb_gpu_active(void);

/* Human-readable refusal/fallback reason (stable string), or "" if none. */
const char *fb_gpu_reason(void);

/* Render+present the current CPU-side frame/state via the GPU path when active.
 * On the live path this MUST NOT read back. Returns 1 on presented, 0 on not
 * handled/failed (caller falls back to CPU present path). */
int fb_gpu_present_frame(const uint32_t *argb, int w, int h);

/* On-demand readback for dump/compare only (never used on the live present path).
 * Copies the latest GPU frame into out_rgba (w*h uint32_t 0xAARRGGBB) or returns
 * 0 if unavailable. */
int fb_gpu_readback(uint32_t *out_rgba, int w, int h);

/* Compare-mode hook: when SH2_FB_GPU_PROVE=compare, compare GPU vs CPU/oracle
 * reference buffer (w*h). Returns 1 if equivalent within policy, 0 if mismatch or
 * compare unavailable. mismatch_count/max_abs/mean_abs set when non-NULL. */
int fb_gpu_compare(const uint32_t *ref_rgba, int w, int h,
                   int *mismatch_count, int *max_abs, double *mean_abs);

/* Upload textures/uniforms for the rb_* raster parity path (Phase 2 scene/batch).
 * Pointers/lengths are caller-owned for the call; implementation copies as needed.
 * Returns 1 on accepted, 0 on refused (CPU fallback). */
int fb_gpu_upload_textures(const void *r, const void *g, const void *b,
                           const void *a, const int *off, const int *sz,
                           int slot_count, int total_texels);
int fb_gpu_set_uniforms(int crt_on, int corrupt_on, int view_w, int view_h);

/* Log present/readback/compare/fallback counters to SH2_FB_GPU_LOG (file). */
void fb_gpu_log_counters(void);

/* Shutdown/release GPU resources (idempotent). */
void fb_gpu_shutdown(void);

#endif /* FB_GPU_H */
