/* fbsound.h — sound for the native fb0 path.
 *
 * THE GAME'S SOUND IS TWO DEVICES' WORTH OF SURFACE, and they are not
 * the same thing:
 *
 *   fb_note(NOTE DURATION)   one oscillator blip. The game writes
 *                            `echo "C3 0.15" > /dev/audio/note` — a
 *                            note NAME and a duration in seconds, and
 *                            the device decides the waveform and the
 *                            envelope. This is the DEFAULT mode
 *                            (SOUND_MODE=notes) and it is a live
 *                            oscillator, not a pre-rendered buffer.
 *
 *   fb_samples(PAYLOAD)      a pre-generated PCM buffer. The game's
 *                            bash sound mode caches a TSV per sound
 *                            and cats it at /dev/audio/samples. The
 *                            payload is
 *                              #sound NAME RATE seed SEED samples N s…
 *                            and the device plays the N samples. This
 *                            is the sample-accurate path: the samples
 *                            ARE the sound, so nothing here may
 *                            resample or re-synthesise them.
 *
 * WHY A DEVICE AT ALL, rather than letting the game shell out to
 * aplay. Two reasons, and the second is the one that matters. The
 * first is the same as the rest of this port: a subprocess per sound
 * is a fork and an exec on the game's hot path, and a player process
 * that outlives the note is a process the game then waits for. The
 * second is that `cat file > /dev/audio/samples` has no timing
 * relationship to the frame that triggered it — the sound lands
 * whenever the process gets to it, which is not the frame the player
 * saw. A blip is a call.
 *
 * ALSA, LINKED LAZILY, so a program that names an fb_* primitive still
 * builds on a box with no sound at all — every framebuffer box, every
 * container, every CI runner. libasound is dlopen'd on the first sound
 * rather than at link time, which is the same discipline fbwin.c uses
 * for libX11 and for the same reason: a missing optional device must
 * cost one failed probe, not a build failure on a machine that will
 * never make a sound.
 *
 * A BOX WITH NO SOUND IS NOT AN ERROR. fb_note returns 0 and does
 * nothing, and the game plays on in silence. A device that failed the
 * process on a machine with no sound hardware would make the fb0 port
 * unusable on exactly the machines it is for.
 */
#ifndef FBSOUND_H
#define FBSOUND_H

/* How long fb_note's oscillators live, in seconds. A game blip is tens
 * of milliseconds; the tail exists so a decaying note is not cut off
 * mid-envelope, and the mixer reaps finished voices on its own. */
#define FB_SOUND_VOICES 16

/* One oscillator blip: `note` is a name like "C3" or "F#4" and `secs`
 * its duration. Returns 1 if the blip was queued, 0 if there is no
 * audio device — which is NOT a failure. */
int fb_note(const char *note, double secs);

/* Play a pre-generated PCM payload (see the header). Returns 1 if it
 * was queued, 0 otherwise. A payload that does not parse plays
 * nothing and says so on stderr once. */
int fb_samples(const char *payload);

/* Non-zero once an audio device has actually been opened. A game can
 * use this to warn ONCE ("no sound card") rather than looking broken,
 * and a test can tell "no audio hardware here" from "the parse is
 * wrong" — which, on this host, is the difference between the two
 * answers the device can give. */
int fb_sound_ready(void);

/* The number of blips and sample buffers played since the device was
 * first used. Diagnostics only; a game must not branch on it. */
int fb_sound_count(void);

/* How many voices are currently sounding. A game that wants a
 * concurrency cap (or a test that wants to wait for silence) asks
 * here. */
int fb_sound_active(void);

/* A NOTE NAME'S FREQUENCY, in Hz, with `*ok` set non-zero when the
 * name was read. 0.0 and *ok = 0 for a name that is not a note.
 *
 * This is public because it is the part of the device with a claim on
 * being CORRECT rather than merely functional: it is a transcription
 * of the audio device's own noteToFreq, and a game in the wrong key is
 * a game that sounds wrong with nothing visible being wrong. A test
 * that could not ask the device what frequency it thinks "C#4" is
 * would be a test that could only check it made a noise. */
double fb_note_freq(const char *name, int *ok);

/* Convert ONE payload sample to int16, the way fb_samples does.
 *
 * Public for the same reason fb_note_freq is: the clamp is the whole
 * content of this conversion and it is INVISIBLE from the outside. A
 * 40000 in a payload becomes -25536 when it wraps, and the sample still
 * plays, the call still returns 1, and the counter still moves — so
 * every observable said the device was fine while the sound was
 * inverted for exactly those samples. */
int fb_sample_clamp(long v);
#endif /* FBSOUND_H */
