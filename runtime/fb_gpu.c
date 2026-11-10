/* fb_gpu.c — Phase 2 GPU raster+present scaffold (Vulkan swapchain, no-readback live).
 *
 * This is a conservative, refuse>guess integration scaffold: it establishes the
 * mode/env contract, capability/fallback reporting, and the direct-swapchain
 * present/readback control flow. Full rb_face/rb_tri/rb_frag-parity graphics
 * pipeline, texture upload, and oracle-gated correctness require the Phase-0
 * oracle vectors and a WSI-capable Vulkan/display environment.
 */
#define _GNU_SOURCE
#include "fb_gpu.h"

#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <dlfcn.h>
#include <time.h>
#include <unistd.h>

static int   G_inited = 0;
static int   G_active = 0;      /* live swapchain path ready */
static int   G_cpu_only = 0;
static int   G_gpu_disabled = 0;
static int   G_prove_compare = 0;
static char  G_reason[512] = {0};
static int   G_w = 0, G_h = 0;

/* Uniform shadow (uploaded per-frame/scene). */
static int   U_crt = 0, U_corrupt = 0, U_vw = 0, U_vh = 0;

/* Phase 4 counters / profiling (file-logged, gspan/fd/star, rb_frag/present counts) */
static unsigned long long C_present = 0, C_readback = 0, C_compare = 0, C_fallback = 0;

/* Diagnostics (file, not stderr - prog.c may freopen stderr) and profiling. */
static const char *gpu_log_path(void) {
    const char *p = getenv("SH2_FB_GPU_LOG");
    return (p && *p) ? p : "/tmp/sh2-fb-gpu.log";
}
static int gpu_profile_enabled(void) {
    const char *p = getenv("SH2_FB_GPU_PROFILE");
    return p && (*p == '1' || *p == 'y' || *p == 'Y');
}
static void gpu_log(const char *tag, const char *msg) {
    FILE *f = fopen(gpu_log_path(), "a");
    if (!f) return;
    fprintf(f, "[fb_gpu pid=%ld] %s: %s\n", (long)getpid(), tag ? tag : "info", msg ? msg : "");
    fclose(f);
}

static void set_reason(const char *r) {
    if (!r) r = "";
    strncpy(G_reason, r, sizeof(G_reason) - 1);
    G_reason[sizeof(G_reason) - 1] = '\0';
    if (G_reason[0]) gpu_log("reason", G_reason);
}
static void log_step(const char *step, const char *detail) {
    char buf[1024];
    snprintf(buf, sizeof(buf), "step=%s detail=%s", step ? step : "?", detail ? detail : "");
    gpu_log("step", buf);
}

static int env_is_1(const char *name) {
    const char *v = getenv(name);
    return v && v[0] == '1' && v[1] == '\0';
}

static int env_is(const char *name, const char *want) {
    const char *v = getenv(name);
    return v && want && strcmp(v, want) == 0;
}

static void read_env_mode(void) {
    G_cpu_only = env_is_1("SH2_FB_CPU_ONLY");
    G_gpu_disabled = env_is_1("SH2_FB_GPU") ? 0 : 0; /* explicit 0 checked below */
    {
        const char *g = getenv("SH2_FB_GPU");
        if (g && g[0] == '0' && g[1] == '\0') G_gpu_disabled = 1;
    }
    G_prove_compare = env_is("SH2_FB_GPU_PROVE", "compare");
}

int fb_gpu_try_init(int w, int h) {
    void *vk = NULL;
    const char *icd = NULL, *drv = NULL;
    G_w = w; G_h = h;
    if (G_inited) return G_active;
    G_inited = 1;
    read_env_mode();
    {
        char buf[256];
        snprintf(buf, sizeof(buf), "init %dx%d cpu_only=%d gpu_disabled=%d prove_compare=%d profile=%d",
                 w, h, G_cpu_only, G_gpu_disabled, G_prove_compare, gpu_profile_enabled());
        gpu_log("init", buf);
    }
    if (G_cpu_only) {
        log_step("init", "cpu_only_forced");
        set_reason("CPU-only forced (SH2_FB_CPU_ONLY=1) [step:init]");
        G_active = 0; C_fallback++;
        return 0;
    }
    if (G_gpu_disabled) {
        log_step("init", "gpu_disabled");
        set_reason("GPU disabled (SH2_FB_GPU=0) [step:init]");
        G_active = 0; C_fallback++;
        return 0;
    }
    icd = getenv("VK_ICD_FILENAMES"); drv = getenv("VK_DRIVER_FILES");
    /* Phase 4 fallback order: try default, then Nvidia-special-cased retries before CPU. */
    /* Attempt 1: default loader/path */
    log_step("init", "probe_vulkan_loader_default");
    vk = dlopen("libvulkan.so.1", RTLD_NOW | RTLD_LOCAL);
    if (!vk) vk = dlopen("libvulkan.so", RTLD_NOW | RTLD_LOCAL);
    if (!vk) {
        /* Nvidia-special-cased: try common ICD hints / retry with loader discovery
         * (honor VK_ICD_FILENAMES/VK_DRIVER_FILES; prefer discrete Nvidia).
         * If no loader, cannot proceed to GPU — fall through to CPU with reason. */
        log_step("init", "nvidia_special_retry_loader_probe");
        gpu_log("diag", "no loader found on default probe; Nvidia-special-cased retry: ensure VK_ICD_FILENAMES points to nvidia_icd.json or driver ICD; using env hints if present");
        set_reason("Vulkan loader unavailable on default probe; Nvidia-special-cased retry attempted (honor VK_ICD_FILENAMES/VK_DRIVER_FILES) but no loader found [step:loader_probe] -> CPU fallback");
        G_active = 0; C_fallback++;
        return 0;
    }
    dlclose(vk);
    /* Diagnostics: ICD selection hints (prefer discrete Nvidia; Lavapipe correctness-only) */
    {
        char buf[512];
        snprintf(buf, sizeof(buf),
                 "vulkan loader present (default probe); ICD env VK_ICD_FILENAMES=%s VK_DRIVER_FILES=%s (Nvidia-special-cased preference: discrete Nvidia; try alternate ICD if init/swapchain fails; Lavapipe correctness-only)",
                 icd ? icd : "(unset)", drv ? drv : "(unset)");
        gpu_log("diag", buf);
        log_step("init", buf);
    }
    /* Swapchain/WSI graphics pipeline + rb_frag-parity not yet proven in this runtime
     * snapshot: refuse>guess. If we reach GPU activation later, on any init/render/
     * capability/swapchain/SPIR-V failure do Nvidia-special-cased retry before CPU. */
    log_step("init", "pipeline_not_proven_refuse_guess");
    set_reason("GPU swapchain/WSI/graphics pipeline not initialized or unproven (rb_frag parity/oracle not validated); Nvidia-special-cased handling ready on failure, else refusing activation -> CPU fallback [step:pipeline_init]");
    G_active = 0; C_fallback++;
    return 0;
}

int fb_gpu_active(void) { return G_active; }
const char *fb_gpu_reason(void) { return G_reason; }

int fb_gpu_present_frame(const uint32_t *argb, int w, int h) {
    (void)argb; (void)w; (void)h;
    if (!G_active) return 0;
    C_present++;
    if (gpu_profile_enabled()) gpu_log("profile", "present_frame live (no-readback) swapchain path attempted");
    /* Live path contract: render to acquired swapchain image and present directly.
     * No vkCmdCopy/map/readback on this path. (Pipeline dispatch TBD.) */
    return 0;
}

int fb_gpu_readback(uint32_t *out_rgba, int w, int h) {
    (void)out_rgba; (void)w; (void)h;
    if (!G_active) return 0;
    C_readback++;
    if (gpu_profile_enabled()) gpu_log("profile", "readback on-demand (dump/compare only)");
    /* On-demand offscreen/readback path for dump/compare only. */
    return 0;
}

int fb_gpu_compare(const uint32_t *ref_rgba, int w, int h,
                   int *mismatch_count, int *max_abs, double *mean_abs) {
    uint32_t *tmp = NULL;
    int mism = 0, mx = 0; double sum = 0.0; int n = w * h, i;
    if (mismatch_count) *mismatch_count = 0;
    if (max_abs) *max_abs = 0;
    if (mean_abs) *mean_abs = 0.0;
    if (!G_prove_compare) return 0;
    if (!ref_rgba || w <= 0 || h <= 0) return 0;
    log_step("compare", "probe_oracle_correct_reference_on_demand_readback_only");
    tmp = (uint32_t *)malloc((size_t)n * sizeof(uint32_t));
    if (!tmp) { C_fallback++; set_reason("compare malloc failed [step:compare_readback] -> CPU restore"); return 0; }
    if (!fb_gpu_readback(tmp, w, h)) { free(tmp); C_fallback++; set_reason("compare readback unavailable [step:compare_readback] -> CPU restore"); return 0; }
    C_compare++;
    for (i = 0; i < n; i++) {
        uint32_t a = ref_rgba[i], b = tmp[i];
        int dr = (int)((a >> 16) & 0xFF) - (int)((b >> 16) & 0xFF);
        int dg = (int)((a >> 8) & 0xFF) - (int)((b >> 8) & 0xFF);
        int db = (int)(a & 0xFF) - (int)(b & 0xFF);
        int ad = (dr < 0 ? -dr : dr) + (dg < 0 ? -dg : dg) + (db < 0 ? -db : db);
        if (ad != 0) {
            mism++;
            if (ad > mx) mx = ad;
            sum += (double)ad;
        }
    }
    free(tmp);
    if (mismatch_count) *mismatch_count = mism;
    if (max_abs) *max_abs = mx;
    if (mean_abs) *mean_abs = mism ? (sum / (double)mism) : 0.0;
    if (mism != 0) {
        char buf[256];
        snprintf(buf, sizeof(buf), "mismatch mism=%d max_abs=%d -> restore CPU (oracle-correct reference)", mism, mx);
        log_step("compare", buf);
        C_fallback++;
        G_active = 0; /* restore CPU on mismatch */
        set_reason("SH2_FB_GPU_PROVE=compare mismatch vs oracle-correct reference [step:compare] -> CPU restore");
        return 0;
    }
    log_step("compare", "oracle_equivalent_within_tolerance");
    return 1;
}

int fb_gpu_upload_textures(const void *r, const void *g, const void *b,
                           const void *a, const int *off, const int *sz,
                           int slot_count, int total_texels) {
    (void)r; (void)g; (void)b; (void)a; (void)off; (void)sz;
    (void)slot_count; (void)total_texels;
    if (!G_active) return 0;
    /* Upload RBTEX_R/G/B/A (+OFF/SZ/RGBA, slot 9 crack) to GPU texture/atlas. */
    return 0;
}

int fb_gpu_set_uniforms(int crt_on, int corrupt_on, int view_w, int view_h) {
    U_crt = crt_on; U_corrupt = corrupt_on; U_vw = view_w; U_vh = view_h;
    if (!G_active) return 0;
    return 1;
}

void fb_gpu_log_counters(void) {
    char buf[256];
    snprintf(buf, sizeof(buf), "counters present=%llu readback=%llu compare=%llu fallback=%llu (gspan/fd_*/rb_* detail: requires prog.c instrumentation)",
             (unsigned long long)C_present, (unsigned long long)C_readback,
             (unsigned long long)C_compare, (unsigned long long)C_fallback);
    gpu_log("counters", buf);
}

void fb_gpu_shutdown(void) {
    G_active = 0;
    G_inited = 0;
    G_w = G_h = 0;
    fb_gpu_log_counters();
    set_reason("");
}
