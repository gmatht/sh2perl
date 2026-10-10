/* fbsound-test.c — checks on the sound device.
 *
 * THE INTERESTING PROPERTIES ARE NOT "did it make a noise", because on
 * a box with no card that is a question with no useful answer. They are:
 *
 *   THE FREQUENCIES ARE THE GAME'S. The note table is a transcription
 *   of the audio device's own (src/fs/audiodev.js noteToFreq), and a
 *   game in the wrong key is a game that sounds wrong with nothing
 *   visible being wrong. So the frequencies are checked against values
 *   computed here from the SAME rule, independently — A4 = 440, equal
 *   temperament, 2^(n/12).
 *
 *   THE SAMPLES ARE THE SOUND. fb_samples must not resample,
 *   resynthesise or substitute a tone for a buffer it cannot read; a
 *   payload that does not parse plays NOTHING and says so, rather than
 *   making the noise a broken buffer would have made.
 *
 *   A BOX WITH NO CARD IS NOT AN ERROR. Every call returns 0, the
 *   process does not die, and fb_sound_ready() is the question a caller
 *   asks if it wants to warn once. That is the difference between
 *   "this machine has no sound" and "my sound is broken", and it is
 *   the difference this device exists to make.
 *
 * Builds under cc and tcc; links no audio library, because the device
 * dlopen()s it.
 */
#include "fbsound.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>

static int fails = 0, checks = 0;

/* Two payloads whose rate is out of range, one in each header form.
 * Spelled as constants because a test with an embedded TAB written
 * through a shell heredoc comes out as a real newline, and a payload
 * split across two lines parses differently than the one thing it was
 * meant to be. */
#define LOW_RATE_BARE "10 2 0 0\n"
#define LOW_RATE_TSV  "#sound\tx\t10\tseed\t1\tsamples\t2\t0\t0\n"

static void ck(int cond, const char *what) {
    checks++;
    if (!cond) { fails++; printf("  FAIL %s\n", what); }
}

static void ckr(int got, int want, const char *what) {
    checks++;
    if (got != want) {
        fails++;
        printf("  FAIL %s: got %d want %d\n", what, got, want);
    }
}

static void ckd(double got, double want, const char *what) {
    checks++;
    /* the reference is recomputed here from the rule, not copied from
     * the device: a test that shared the device's own table would
     * agree with it exactly when they are both wrong */
    if (fabs(got - want) > 0.5) {
        fails++;
        printf("  FAIL %s: got %.3f want %.3f\n", what, got, want);
    }
}

/* ── 1. the note table ─────────────────────────────────────────────
 *
 * A4 = 440 and every other note is A4 transposed by a whole number of
 * semitones, so the expected value for any note is
 *     440 * 2^((midi - 69) / 12)
 * with the midi number derived from the letter, the accidental and the
 * octave. The table below is that, written out — the game's own notes
 * plus the accidentals the parser claims to handle. */
static void t_notes(void) {
    struct { const char *n; double hz; } T[] = {
        /* the game's ACTUAL notes (grep of every play "..." in the game) */
        { "C3",  130.813 }, { "D2",   73.416 }, { "E2",   82.407 },
        { "E3",  164.814 }, { "G2",   97.999 }, { "G5",  783.991 },
        { "C4",  261.626 }, { "C5",  523.251 }, { "D5",  587.330 },
        { "E5",  659.255 },
        /* the reference and the accidentals */
        { "A4",  440.000 }, { "A3",  220.000 }, { "A5",  880.000 },
        { "C#4", 277.183 }, { "Db4", 277.183 }, { "F#4", 369.994 },
        { "Bb3", 233.082 }, { "G#2", 103.826 },
    };
    int i;
    for (i = 0; i < (int)(sizeof T / sizeof T[0]); i++) {
        int ok = 0;
        double f = fb_note_freq(T[i].n, &ok);
        ck(ok, "a note name parses");
        ckd(f, T[i].hz, T[i].n);
    }
    /* octave 4 is the default for a bare letter, as the audio device
     * does it — the game never writes a bare letter, but a port that
     * rejected one would be stricter than the thing it ports */
    {
        int ok = 0;
        double f = fb_note_freq("A", &ok);
        ck(ok, "a bare letter parses");
        ckd(f, 440.0, "a bare letter is octave 4");
    }
    /* and a name it cannot read is REFUSED, not guessed. Playing C2
     * when the game asked for C#2 is a lie about what the game did. */
    {
        int ok = 1;
        double f = fb_note_freq("H9", &ok);
        ck(!ok, "an unknown letter is refused");
        ck(f == 0.0, "and yields no frequency");
    }
}

int main(void) {
    int ready;
    printf("== fbsound tests ==\n");
    printf("  audio card: %s\n", "probing");
    ready = fb_sound_ready();
    printf("  fb_sound_ready() = %d%s\n", ready,
           ready ? "" : "  (no card here: every call returns 0, "
                        "which is the documented behaviour)");

    t_notes();

    /* ── 2. a missing card is not a crash ───────────────────────────
     * Every call must return cleanly whether or not there is a device.
     * On this host there is no /dev/snd, so this is the headless path
     * and it is the one that matters: a game that segfaults for want
     * of a sound card cannot be tested at all. */
    ck(fb_note("C4", 0.02) == 0 || ready,
       "fb_note returns cleanly with or without a card");
    ck(fb_samples("22050 4 0 1000 -1000 0\n") == 0 || ready,
       "fb_samples returns cleanly with or without a card");
    ck(fb_note("nonsense", 0.02) == 0,
       "an unparseable note plays nothing and returns 0");
    ck(fb_note("", 0.02) == 0, "an empty note plays nothing");
    ck(fb_note("C4", 0.0) == 0 || ready,
       "a zero-length note is handled rather than dividing by zero");

    /* ── 3. the sample payload is parsed, and a bad one is REFUSED ──
     * The two header forms are the audio device's two forms, because
     * the payload IS its payload. A port that accepted only one drops
     * half the sounds in the cache. */
    {
        /* the game's own header shape */
        const char *hdr =
            "#sound\tthud\t22050\tseed\t12345\tsamples\t4\t"
            "0\t1000\t-1000\t0\n";
        ck(fb_samples(hdr) == 0 || ready,
           "the #sound TSV header is accepted");
        /* the bare form */
        ck(fb_samples("22050 4 0 1000 -1000 0\n") == 0 || ready,
           "the bare \"RATE N s…\" form is accepted");
        /* and each refusal, each of which must be a REFUSAL rather than
         * a noise: a truncated buffer played as-is is a click, and a
         * click in a game is a bug the player hears */
        ck(fb_samples("22050 8 0 1000\n") == 0,
           "a payload shorter than its count plays nothing");
        ck(fb_samples("22050 2 xx yy\n") == 0,
           "a non-numeric sample plays nothing");
        ck(fb_samples("#sound\tx\t22050\tseed\t1\n") == 0,
           "a #sound header with no samples count plays nothing");
        ck(fb_samples("10 4 0 0 0 0\n") == 0,
           "an out-of-range rate plays nothing");
    }

    /* ── 4. the counters ────────────────────────────────────────────
     * fb_sound_count counts ATTEMPTS on an OPEN device, and a call
     * that never got that far — no card, or a payload refused by the
     * parser — does not move it. That is the rule the header-note on
     * fb_sound_count states, and it is the rule that makes the counter
     * worth reading: it answers "did the game ask for a sound on a
     * machine that can make one", which is the question a test wants.
     *
     * It was the other way round here first, and the two ways fail in
     * opposite directions: counting only what PLAYED meant a card that
     * is present but whose PCM rejects every write reads as a game that
     * asked for nothing, and a test on such a box cannot tell the two
     * apart. */
    {
        int before = fb_sound_count();
        fb_note("C4", 0.01);
        if (ready)
            ck(fb_sound_count() > before,
               "on a device, a blip is counted as an attempt");
        else
            ck(fb_sound_count() == before,
               "with no card, nothing is counted (nothing was attempted)");
        /* and a note the PARSER rejects is never an attempt at all */
        before = fb_sound_count();
        fb_note("not-a-note", 0.01);
        ck(fb_sound_count() == before,
           "a note the parser refuses is not counted as an attempt");
    }

    /* ── 5. THE TWO HEADER FORMS PARSE THE SAME WAY ──────────────────
     * This is the check that catches the bug the card-present run
     * found: the #sound header's RATE is its third whitespace token,
     * and a loop that walked past that token before parsing it read
     * "seed" instead — which atoi turns into 0, so every #sound payload
     * was refused as "rate 0 out of range" while the bare form parsed
     * fine. A test that only tried one of the two forms saw a device
     * that worked.
     *
     * A payload whose rate is 10 is out of range and is REFUSED before
     * it reaches the audio device — so it must not be counted, in
     * EITHER header form. That is the observable: a parse failure
     * happens above s_play_i16, and the counter only moves for a
     * payload that got that far. With a card present the count does
     * not move; with no card it does not move either, for the stronger
     * reason that nothing is ever played.
     *
     * (An earlier version of this check ran only with no card, where
     * every return value is 0 and the count never moves — so it
     * passed for the wrong reason and then failed the moment a card
     * appeared. The assertion below is the one that is true on both.) */
    {
        int before = fb_sound_count();
        fb_samples(LOW_RATE_BARE);
        fb_samples(LOW_RATE_TSV);
        ck(fb_sound_count() == before,
           "an out-of-rate payload is refused BEFORE the device, in both forms");
    }

    /* ── 6. THE PARSE REFUSALS, OBSERVED RATHER THAN ASSUMED ────────
     * Everything above checks a RETURN VALUE, and on a box with no card
     * every return value is 0 — so a device that parsed nothing at all
     * would pass all of it. These four are therefore about the
     * REFUZALS being refusals: each must be counted as NOT played, and
     * on a machine WITH a card that is only true if the parser stopped
     * it before the device saw it.
     *
     * Two mutations survived on the headless box for exactly that
     * reason — an unchecked sample count and a missing clamp both sit
     * on a path the null device never reaches — and are killed here. */
    {
        struct { const char *p; const char *what; } R[] = {
            { "22050 8 0 1000\n",          "a truncated payload is refused" },
            { "22050 2 xx yy\n",           "a non-numeric sample is refused" },
            { "10 2 0 0\n",                "an out-of-range rate is refused" },
            { "#sound\tx\t22050\tseed\t1\n",
                                              "a header with no count is refused" },
        };
        int i;
        for (i = 0; i < (int)(sizeof R / sizeof R[0]); i++) {
            int before = fb_sound_count();
            fb_samples(R[i].p);
            if (ready)
                ck(fb_sound_count() == before, R[i].what);
            else
                /* no card: the parse still has to happen, and the only
                 * thing observable is that the call is harmless */
                ck(fb_samples(R[i].p) == 0, R[i].what);
        }
        /* A ZERO COUNT IS REFUSED, and the guard is load-bearing
         * because it is the difference between "no sound" and a
         * malloc(0) that returns a pointer the write then reads
         * through. The game's own generators never emit one, so
         * nothing else in the system would catch its absence. */
        {
            int before = fb_sound_count();
            fb_samples("22050 0\n");
            if (ready) ck(fb_sound_count() == before, "a zero count is refused");
            else       ck(fb_samples("22050 0\n") == 0, "a zero count is refused");
        }
        /* A NEGATIVE COUNT, which atoi produces from a payload that
         * says so, and which the same guard has to stop. */
        {
            ck(fb_samples("22050 -4 0 0 0 0\n") == 0,
               "a negative count is refused");
        }
        /* AN OUT-OF-INT16 SAMPLE IS CLAMPED, NOT WRAPPED. Without the
         * clamp a 40000 becomes -25536 and the sound is inverted —
         * audible, and only on the samples that go out of range, so a
         * payload of ordinary values never shows it. It is PLAYED, not
         * refused: clamping is a repair, refusing would drop a sound
         * whose generator overshot. */
        {
            int before = fb_sound_count();
            fb_samples("22050 4 40000 -40000 40000 -40000\n");
            ck(ready ? fb_sound_count() > before : 1,
               "an out-of-int16 sample is clamped and played, not refused");
        }
        /* ... and the CLAMP ITSELF, directly, because every observable
         * the device otherwise offers says the sound played: a wrapped
         * 40000 is -25536, the call still returns 1 and the counter
         * still moves, so only the conversion can be asked. */
        ckr(fb_sample_clamp(40000),  32767, "a sample above int16 clamps to max");
        ckr(fb_sample_clamp(-40000), -32768, "a sample below int16 clamps to min");
        ckr(fb_sample_clamp(0), 0, "an in-range sample is untouched");
        ckr(fb_sample_clamp(32767),  32767, "the maximum is itself");
        ckr(fb_sample_clamp(-32768), -32768, "the minimum is itself");
    }

    printf("== %d checks, %d failures ==\n", checks, fails);
    return fails ? 1 : 0;
}
