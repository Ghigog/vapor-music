# AI DJ Smart Mixing Workflow

This guide details the matching algorithms, sequence logic, and interface controls of the AI DJ "Smart Mixing" engine in Vapor Music.

---

## 1. Mixing Mode Toggle
- **Smart Mixing OFF**: Sequential playback down the active playlist in order (`(current_track_index + 1) % playlist.size()`).
- **Smart Mixing ON**: Automated playback managed by the AI DJ's calculated mood path, unless overridden by manual selection.

---

## 2. The three exits

Every transition is one of three, judged from the track playing to a candidate.
The thresholds are in `exit_between` in `vapor-app/src-tauri/src/lib.rs` and were
measured against a real 534-track library rather than chosen:

- **Stay** — hold this level. Intensity within 0.15 and tempo within 8 BPM, in a
  similar genre. The engine has an easy time here: the transition is a Bass Swap
  or a Filter Sweep.
- **Follow** — carry on. This is not a classification at all: it is whatever the
  planner (§7) has queued next, which is the whole point of the shape.
- **Switch** — leave this level. A different genre, or intensity 0.30 apart, or
  45 BPM apart. Key compatibility matters less because the transition is masked
  by an effect (Echo Out / Reverb Freeze).

The original's classes were Match / Fresh / Switch, where Fresh meant "similar
genre, deliberately about 15 BPM and 0.25 energy away". Fresh has become Follow
because the two were doing the same job badly: Fresh proposed a track the set
had not planned, so the suggestion and the queue could disagree about what was
coming next, and the screen showed a badge for one and a highlight for the
other.

### Filling the three

Stay and Switch are searched in that order, over everything analysed except the
track playing, what has already played, and the Follow track.

Stay is asked of *every* candidate — "how little does the level move", with
transition cost breaking ties — not only of the ones inside the band above. A
library holding nothing within 8 BPM still has a closest track, and offering two
cards because the third missed a threshold is the screen withholding an answer
it has. Switch is then taken from what Stay left: among the genuine departures
if there are any, and otherwise the furthest remaining track.

So the screen shows three cards whenever there are three tracks to fill them.

---

## 3. What happens if nobody presses anything

Follow. The set carries on.

The original cycled Match → Fresh → Match → Switch and called whichever step it
was on the "AI Choice". That cycle is gone. It made the default answer depend on
a hidden counter, so the same pair of tracks got a different verdict depending on
when you arrived, and the counter advanced on manual overrides too — which meant
overruling one transition silently changed the next one.

---

## 4. UI denotations

- **One mark, on the queued track.** The card that is queued next carries a ring
  in its own colour. There is no second mark.
- **Taking an exit folds it into the set.** Pressing Stay or Switch queues that
  track and re-plans the tail from it, so on the next render it *is* the Follow
  card.
- **Colour carries the exit.** Stay is the sovereignty green, Follow the app
  accent, Switch amber. The word is on the sleeve as well, because colour alone
  is not an accessible signal.

There used to be a `🤖 AI Choice` badge that stayed on the DJ's own pick while
the highlight moved to a manual override. Two marks answering one question is a
screen disagreeing with itself, and it is gone with the cycle that fed it.

---

## 5. Transition Effects
The AI DJ automatically selects from 6 transition types based on the BPM difference and the key relationship (harmonic, modulated, or clashing) to mimic how a real DJ plays. Transition durations are phrase-adaptive: if outro/intro segment metadata is available for both tracks, the duration is dynamically set as the overlap of the segments, quantized to standard musical phrase boundaries (16, 8, or 4 bars) based on the outgoing track's BPM, and clamped between `4.0s` and `16.0s` (falling back to `4.0s` if no standard phrase fits). Otherwise, it falls back to the transition type's default duration.

Transition loading is triggered `duration + 4.0` seconds before the track ends, and begins exactly at `duration` seconds remaining:

- **Bass Swap** (BPM diff < 3.0, Harmonic or Modulated): 6.0s blend. Low EQ frequencies crossfade smoothly around the midpoint to prevent abrupt energy cuts.
- **Filter Sweep** (BPM diff 3.0–8.0, Harmonic): 4.0s blend. Outgoing lowpass and incoming highpass sweeps.
- **Tempo Morph** (BPM diff 3.0–8.0, Modulated/Clashing): 6.0s blend. Syncs tempos during crossfade, then ramps to native tempo.
- **Reverb Freeze** (BPM diff < 8.0, Clashing / Switch): 5.0s blend. Outgoing reverb freezes at midpoint to wash out the clashing frequencies and mask the transition.
- **Echo Out** (BPM diff >= 8.0, Clashing/Modulated or Fresh/Switch): 5.0s blend. Outgoing delay rings out from midpoint to mask key clashes and major BPM jumps.
- **Standard Crossfade** (BPM diff >= 8.0, Harmonic): 3.0s blend. Fast linear volume crossfade.

---

## 6. Mix Tuner & Vibe Limit
- **Vibe Limit**: Sets the maximum energy difference allowed between consecutive tracks.
- **Strict (Low Value)**: Restricts the AI DJ to very smooth transitions with consistent energy, keeping the overall vibe stable.
- **Loose (High Value)**: Permits larger energy shifts between tracks, allowing for dramatic drops and climbs in set intensity.

---

## 7. Conduct a Set (the Mood Path)

This is the other half of the DJ, and it was never reachable from the Godot UI —
`play_harmonic_shuffle()` called `DJPathfinder.generate_mood_path()` with two
arguments, so `target_curve` always fell to its default of `"build"`. The other
three curves were implemented and unreachable. The Vibe screen exposes all four.

Where §2–§4 decide **which track comes next**, this decides **where the whole
set is going**. It plans the running order in advance with an A\* search over
the Camelot wheel, scoring each candidate on two things at once:

- **Transition cost** — the same harmonic and tempo distance used everywhere
  else, so consecutive tracks stay mixable.
- **Curve cost** — how far a candidate sits from where the set is *supposed* to
  be by that point.

### The curves

For step `i` of `N`, with `t = i / (N - 1)`, the target is:

| Curve | Target energy | Target tempo |
|---|---|---|
| **Build Vibe** | start → start + 0.4 | start → start + 15 BPM |
| **Chill Down** | start → start − 0.4 | start → start − 15 BPM |
| **Wave** | start + 0.3·sin(2πt) | start + 10·sin(2πt) |
| **Hold Steady** | start, unchanged | start, unchanged |

Build and Chill are linear ramps across the set. Wave completes one full cycle —
up, back through the middle, down, and home. Hold Steady sets a flat target, so
only transition cost decides the order.

Energy is integrated loudness, mapped to 0–1 over −30 to −5 LUFS.

It was a dynamics ratio — mean RMS over peak RMS — until 2026-08-17. That
measures how *consistent* a track is rather than how hard it goes: one that sits
at a single level scores high and one with a breakdown scores low. Measured on a
real 534-track library it put drum & bass at 0.661 against 0.629 for ballads,
ranges overlapping completely, with the quietest records at the top. It was
deciding the curves, the energy term in the transition cost, and whether two
tracks count as a match.

Loudness separates the same two groups by 0.256 instead of 0.031. The spec used
to claim energy was "loudness, brightness and tempo in equal parts"; it never
was, and the sentence outlived the code it described.

Tracks with no analysis are not placed, because the cost model has no tempo or
key to place them by. They are appended at the end and the screen says how many.

### How it works with the three exits

The two are layers, not alternatives:

- The **curve** owns the destination — where the set should be at step `i`.
- The **exit** owns the next step — which specific track, and how it is mixed
  in.

The planner queues ten tracks ahead, and the Follow card is the head of that
queue, so the default answer for every transition is simply "carry on". Choosing
Stay or Switch by hand overrides that one step; the remaining plan is then
re-searched from the new track against the same curve, so the set still arrives
where it was going. Being 60% through a Build stays 60% through a Build — the
arc is preserved, the route to it changes.

Choosing a curve does the same thing to the whole tail: everything after the
track playing was a route to the old destination, so it is discarded and
re-planned. There is no "Conduct" button, because there is nothing left for it
to do — the playback thread extends the set along the saved curve whether or not
the screen is open. It used to be the only thing that ever ran the planner,
which is why a set was something you had to know to ask for.
