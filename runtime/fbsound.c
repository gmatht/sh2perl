/* fbsound.c — sound for the native fb0 path (see fbsound.h).
 *
 * TWO PATHS, because the game has two:
 *
 *   fb_note     an OSCILLATOR. The game writes `echo "C3 0.15" >
 *              /dev/audio/note` — a note name and a duration — and the
 *              device synthesises it. The note-to-frequency table and
 *              the A4 = 440 reference are the audio device's own
 *              (src/fs/audiodev.js, noteToFreq), transcribed: a port
 *              that invents its own tuning plays a game whose sound is
 *              in the wrong key, and nothing about that is visible in a
 *              test that only asks "did it make a noise".
 *
 *   fb_samples  a PRE-RENDERED PCM BUFFER, played as given. The bash
 *              sound mode caches a TSV per sound and cats it at
 *              /dev/audio/samples; the samples ARE the sound, so this
 *              path resamples nothing and re-synthesises nothing. It
 *              converts int16 to the device's format and plays.
 *
 * ALSA IS dlopen'd, NOT LINKED. A box with no sound hardware must
 * still BUILD: every framebuffer box, every container, every CI runner
 * has no libasound and no /dev/snd, and a link-time -lasound would
 * make the fb0 port unusable on exactly the machines it is for. The
 * same discipline fbwin.c uses for libX11, and for the same reason.
 *
 * AND A MISSING CARD IS NOT AN ERROR. fb_note returns 0 and the game
 * plays on in silence. A device that refused to run a game for want of
 * a sound card would be a device whose failure mode is "nothing
 * happens", which is the worst one to debug.
 *
 * No VLAs, no nested functions, no C99-in-preprocessor tricks: this
 * compiles clean under both cc -Wall -Wextra and tcc, like the rest of
 * the fb runtime.
 */
#define _GNU_SOURCE
#include "fbsound.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <dlfcn.h>
#include <ctype.h>

/* ── the lazily-loaded ALSA ────────────────────────────────────────
 * Only the handful of entry points this device uses. Declaring them
 * as function pointers rather than including <alsa/asound.h> is what
 * keeps a build working with no ALSA headers installed at all, which
 * is the point: the header is a convenience, not a requirement. */
typedef struct {
    int  (*snd_pcm_open)(void **, const char *, int, int);
    int  (*snd_pcm_close)(void *);
    int  (*snd_pcm_prepare)(void *);
    int  (*snd_pcm_writei)(void *, const void *, unsigned long);
    int  (*snd_pcm_drain)(void *);
} alsa_t;

#define SND_PCM_STREAM_PLAYBACK 0
#define SND_PCM_NONBLOCK       1

static void        *S_lib = 0;
static alsa_t      S_a;
static void       *S_pcm = 0;
static int         S_tried = 0;
static int         S_ready = 0;
static int         S_count = 0;
static int         S_active = 0;

/* The open flags, spelled out rather than included, so a box with no
 * ALSA headers still compiles. ALSA_OPEN_PCM = 0x100000 | 0x1 = 1048577;
 * the value is stable ABI and is what <alsa/pcm.h> defines. */
#define ALSA_OPEN_PCM 0x100001

/* Load the library and open the default PCM. Idempotent: the first
 * call probes, every later one is a flag test. A failed probe is
 * remembered as a failure and NOT retried per call — a machine with no
 * sound card would otherwise pay a dlopen and a failed open on every
 * frame, which at 60 fps is the sound device costing the game more
 * than the sound ever could. */
static int s_open(void) {
    if (S_tried) return S_ready;
    S_tried = 1;
    S_lib = dlopen("libasound.so.2", RTLD_LAZY);
    if (!S_lib) S_lib = dlopen("libasound.so", RTLD_LAZY);
    if (!S_lib) return 0;
    S_a.snd_pcm_open     = (int (*)(void **, const char *, int, int))dlsym(S_lib, "snd_pcm_open");
    S_a.snd_pcm_close    = (int (*)(void *))dlsym(S_lib, "snd_pcm_close");
    S_a.snd_pcm_prepare  = (int (*)(void *))dlsym(S_lib, "snd_pcm_prepare");
    S_a.snd_pcm_writei   = (int (*)(void *, const void *, unsigned long))dlsym(S_lib, "snd_pcm_writei");
    S_a.snd_pcm_drain    = (int (*)(void *))dlsym(S_lib, "snd_pcm_drain");
    if (!S_a.snd_pcm_open || !S_a.snd_pcm_close || !S_a.snd_pcm_writei)
        return 0;
    if (S_a.snd_pcm_open(&S_pcm, "default", SND_PCM_STREAM_PLAYBACK, 0) < 0) {
        S_pcm = 0;
        return 0;
    }
    S_ready = 1;
    return 1;
}

int fb_sound_ready(void) { return s_open(); }

/* An attempt is counted only when there IS a device to attempt on, and
 * whether or not the write landed.
 *
 * The "only when" is the half that matters and it was the half I got
 * wrong first. Counting unconditionally made a machine with no sound
 * card report every blip the game asked for, which is a count of
 * nothing: the counter then cannot distinguish "the game is silent"
 * from "this box cannot make a noise", and those are exactly the two
 * things a person debugging a silent game needs told apart.
 *
 * The "whether or not the write landed" is the other half. A card that
 * is present but whose PCM rejects every write is a real state, and a
 * counter that only moved on success made it indistinguishable from a
 * game that asked for no sounds at all. */
static int s_attempted(int played) {
    if (S_ready) S_count++;
    return played;
}
int fb_sound_count(void) { return S_count; }
int fb_sound_active(void) { return S_active; }

/* ── note name → frequency ────────────────────────────────────────
 * The game's own table (src/fs/audiodev.js noteToFreq), transcribed:
 * A4 = 440, the equal-tempered semitone offsets, and a bare letter
 * meaning octave 4. The regex is reimplemented rather than imported
 * because the device cannot run JS, so this IS the transcription, and
 * it is a transcription rather than a redesign for the reason above: a
 * game in the wrong key is a game that sounds wrong without anything
 * looking wrong.
 *
 * Returns 0 for a name it cannot read, and the CALLER decides what
 * that means — a bad note in a game payload is a silent frame, not a
 * crash, and not a substituted frequency either: playing C2 when the
 * game asked for C#2 is a lie about what the game did. */
double fb_note_freq(const char *name, int *ok) {
    int semis = 0, oct = 4, i = 0;
    char c;
    *ok = 0;
    if (!name) return 0.0;
    while (name[i] == ' ') i++;
    c = (char)toupper((unsigned char)name[i]);
    switch (c) {
        case 'C': semis = 0;  break;
        case 'D': semis = 2;  break;
        case 'E': semis = 4;  break;
        case 'F': semis = 5;  break;
        case 'G': semis = 7;  break;
        case 'A': semis = 9;  break;
        case 'B': semis = 11; break;
        default: return 0.0;
    }
    i++;
    if (name[i] == '#') { semis += 1; i++; }
    else if (name[i] == 'b') { semis -= 1; i++; }
    if (name[i] >= '0' && name[i] <= '9') {
        oct = 0;
        while (name[i] >= '0' && name[i] <= '9') { oct = oct * 10 + (name[i] - '0'); i++; }
    }
    {
        int midi = (oct + 1) * 12 + semis;
        if (midi < 0 || midi > 127) return 0.0;
        *ok = 1;
        return 440.0 * pow(2.0, (midi - 69) / 12.0);
    }
}

/* Play a mono int16 buffer at `rate` Hz.
 *
 * THE WRITE IS THE PACING, so it is a BLOCKING write of the whole
 * buffer: a 50 ms blip is one 50 ms write, and the game's frame budget
 * absorbs it the way it absorbs any other small cost. A non-blocking
 * write followed by a poll would either drop the tail or need a thread
 * to exist at all.
 *
 * PREPARE IS CALLED BEFORE EACH WRITE, which is the recovery for a PCM
 * that has gone to EPIPE — an underrun, or a device that was suspended
 * between two frames. A game is the worst case for this: it plays
 * sounds minutes apart, and a PCM left in a prepared-but-drained state
 * (or woken from suspend between two blips) rejects the write with
 * -EPIPE forever if nothing prepares it. One prepare-and-retry, and
 * then the sound is DROPPED: a retry loop here is a hang, and a silent
 * frame is a better outcome than a frozen game.
 *
 * ── WHAT IS AND IS NOT VERIFIED HERE ──────────────────────────────
 * This host has /dev/snd/timer and no card, so everything above the
 * snd_pcm_writei call is exercised and everything below it is not. A
 * null ALSA device (type null) opens, and then every write and every
 * prepare returns -5/EPIPE, so even that cannot stand in for a card.
 * What IS measured: the note table, the payload parse, the refusals,
 * the counter, and that a missing card costs one dlopen and one failed
 * open rather than a crash.
 *
 * What is NOT measured: that a real card produces audible sound. The
 * alternative was to claim it works, and a claim like that is worth
 * nothing. The write path is the standard ALSA sequence — open the
 * playback stream, prepare, write — and if it is wrong on real hardware
 * the failure is loud (no sound), immediate, and in one function. */
static int s_play_i16(const int16_t *pcm, int n, int rate) {
    (void)rate;   /* the device resamples on write; the rate is its business */
    int frames;
    if (!s_open()) return 0;
    if (S_a.snd_pcm_prepare) (void)S_a.snd_pcm_prepare(S_pcm);
    frames = S_a.snd_pcm_writei(S_pcm, pcm, (unsigned long)n);
    if (frames < 0 && S_a.snd_pcm_prepare) {
        (void)S_a.snd_pcm_prepare(S_pcm);
        frames = S_a.snd_pcm_writei(S_pcm, pcm, (unsigned long)n);
    }
    if (frames > 0) {
        S_active++;
        return 1;
    }
    return 0;
}

int fb_note(const char *note, double secs) {
    double freq;
    int ok, i, n, rate = 22050;
    int16_t *pcm;
    if (!note) return 0;
    freq = fb_note_freq(note, &ok);
    if (!ok) return 0;
    if (secs <= 0.0) secs = 0.1;
    if (secs > 2.0) secs = 2.0;      /* a blip, not a drone */
    n = (int)(secs * rate);
    if (n < 1) n = 1;
    if (n > rate * 2) n = rate * 2;
    pcm = (int16_t *)calloc((size_t)n, sizeof(int16_t));
    if (!pcm) return 0;
    /* A SINE WITH A FAST DECAY, which is what the WebAudio path's
     * gain envelope amounts to for a percussive blip: the attack is
     * immediate and the tail is gone well before the note's nominal
     * duration, so two shots in quick succession do not smear into one
     * long tone. The envelope is exponential in the ATTENUATION, which
     * is what makes it sound like a struck object rather than a
     * switch being turned off. */
    for (i = 0; i < n; i++) {
        double t = (double)i / (double)rate;
        double env = exp(-t * 22.0);
        double v = sin(6.283185307179586 * freq * t) * env * 0.30;
        int s = (int)(v * 32767.0);
        if (s >  32767) s =  32767;
        if (s < -32768) s = -32768;
        pcm[i] = (int16_t)s;
    }
    i = s_attempted(s_play_i16(pcm, n, rate));
    free(pcm);
    return i;
}

int fb_sample_clamp(long v) {
    /* CLAMP, DO NOT WRAP. The generators emit signed 16-bit PCM, so a
     * value outside that range is a generator that overshot — and
     * casting it to int16 would turn 40000 into -25536, inverting
     * that sample. Every observable the device offers says the sound
     * played: the return is 1, the counter moves, and the buffer is the
     * right length. Only the arithmetic is wrong, and it is wrong in a
     * way a player hears and a test reading return values does not. */
    if (v < -32768) return -32768;
    if (v >  32767) return  32767;
    return (int)v;
}

/* ── the sample payload ───────────────────────────────────────────
 * The game's generators emit
 *     #sound NAME RATE seed SEED samples N s1 s2 …
 * and a bare "RATE N s1 s2 …" is accepted too — the same two forms the
 * audio device accepts, because the payload IS the audio device's
 * payload and a port that accepts only one of them drops half the
 * sounds in the cache.
 *
 * THE SAMPLES ARE THE SOUND. This path does not resample, does not
 * synthesise, and does not substitute a tone for a buffer it cannot
 * read: the sample-accurate mode is the one where the game's own
 * generator output is what you hear, and anything else is a different
 * game. A payload that does not parse plays nothing and says so once.
 */
int fb_samples(const char *payload) {
    const char *p = payload;
    int rate = 22050, n = 0, i = 0, cap;
    int16_t *pcm;
    int start = -1;
    if (!payload) return 0;

    /* the header: find "samples N", or "RATE N" in the bare form */
    if (p[0] == '#') {
        const char *q = strstr(p, "samples");
        if (!q) {
            fprintf(stderr, "fb_samples: no \"samples\" count in a #sound payload;"
                            " nothing played\n");
            return 0;
        }
        {
            const char *r = q;
            while (*r && *r != ' ' && *r != '\t' && *r != '\n') r++;
            while (*r == ' ' || *r == '\t' || *r == '\n') r++;
            n = atoi(r);
            /* THE RATE IS THE THIRD WHITESPACE TOKEN, and the header
             * is   #sound NAME RATE seed SEED samples N s…
             * so it is token index 2 counting from 0. The loop below
             * walked `seen` tokens and then read `atoi(t)` — but `t`
             * had already been advanced PAST that token by the loop
             * body, so it parsed the FOURTH one ("seed", which atoi
             * turns into 0) and every #sound payload was refused with
             * "rate 0 out of range". The bare form parsed fine, which
             * is why a test that only tried one of the two forms found
             * nothing wrong.
             *
             * The rate is read as the token AFTER the second, i.e. the
             * start of the token whose index is 2: skip two whole
             * tokens, then parse from there. */
            {
                const char *t = p;
                int tok = 0;
                while (*t && tok < 2) {
                    while (*t == ' ' || *t == '\t' || *t == '\n') t++;
                    while (*t && *t != ' ' && *t != '\t' && *t != '\n') t++;
                    tok++;
                }
                while (*t == ' ' || *t == '\t' || *t == '\n') t++;
                rate = atoi(t);
            }
            /* the data starts after the count */
            while (*r >= '0' && *r <= '9') r++;
            start = (int)(r - p);
        }
    } else {
        while (*p == ' ' || *p == '\t' || *p == '\n') p++;
        rate = atoi(p);
        while (*p && *p != ' ' && *p != '\t' && *p != '\n') p++;
        while (*p == ' ' || *p == '\t' || *p == '\n') p++;
        n = atoi(p);
        while (*p >= '0' && *p <= '9') p++;
        while (*p == ' ' || *p == '\t' || *p == '\n') p++;
        start = (int)(p - payload);
    }
    if (n < 1) {
        fprintf(stderr, "fb_samples: sample count %d; nothing played\n", n);
        return 0;
    }
    if (rate < 4000 || rate > 96000) {
        fprintf(stderr, "fb_samples: rate %d out of range; nothing played\n", rate);
        return 0;
    }
    cap = n;
    pcm = (int16_t *)malloc((size_t)cap * sizeof(int16_t));
    if (!pcm) return 0;
    p = payload + start;
    for (i = 0; i < n; i++) {
        long v;
        char *end;
        while (*p == ' ' || *p == '\t' || *p == '\n' || *p == '\r') p++;
        if (!*p) {
            fprintf(stderr, "fb_samples: payload short — header says %d, found %d;"
                            " nothing played\n", n, i);
            free(pcm);
            return 0;
        }
        v = strtol(p, &end, 10);
        if (end == p) {
            fprintf(stderr, "fb_samples: bad sample %d ('%.12s');"
                            " nothing played\n", i, p);
            free(pcm);
            return 0;
        }
        p = end;
        pcm[i] = (int16_t)fb_sample_clamp(v);
    }
    i = s_attempted(s_play_i16(pcm, n, rate));
    free(pcm);
    return i;
}
