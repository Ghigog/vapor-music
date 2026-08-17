# AI DJ Smart Mixing Workflow

This guide details the matching algorithms, sequence logic, and interface controls of the AI DJ "Smart Mixing" engine in Vapor Music.

---

## 1. Mixing Mode Toggle
- **Smart Mixing OFF**: Sequential playback down the active playlist in order (`(current_track_index + 1) % playlist.size()`).
- **Smart Mixing ON**: Automated playback managed by the AI DJ's calculated mood path, unless overridden by manual selection.

---

## 2. Match Classifications
Instead of arbitrary songs, recommendations are categorized into three distinct profiles based on real-world DJing principles:
- **Match (`perfect`)**: Perfect Match / Continue Vibe. Selects a harmonically compatible track (Exact key match, Mode Shift, adjacent key Harmonic Step, or Diagonal Step; Camelot distance cost <= 2.0) with the lowest overall transition cost.
- **Fresh (`interesting`)**: Interesting Match / Innovate Vibe. Restricts candidates to similar/matching genres and specific harmonic key modulations (Energy Boost +2, Energy Drop -2, Power Fifth Mix +7, Subdominant Mix +5, or Diagonal Step; Camelot distance cost 2.0–3.0) to shift the energy of the room. It targets a change of ~15 BPM and ~0.25 energy levels.
- **Switch (`creative`)**: Creative Match / Switch Genre. Restricts candidates to a different genre, matching the BPM and energy level as closely as possible to keep the rhythm consistent. Key compatibility is ignored since the transition will be masked by high-energy effects (Echo Out / Reverb Freeze).

---

## 3. The AI Choice Sequence
When Smart Mixing is active, the AI DJ automatically cycles through a repeating 4-step sequence (the "mood path"):
- **Step 0**: `perfect` (Match) — Selects the Perfect Match track.
- **Step 1**: `interesting` (Fresh) — Selects the Fresh Match track.
- **Step 2**: `perfect` (Match) — Selects the Perfect Match track.
- **Step 3**: `creative` (Switch) or `interesting` (Fresh) — 50% chance of Switch, 50% chance of Fresh.

---

## 4. UI Denotations
- **AI Choice**: The default selection is highlighted and labeled with a `🤖 AI Choice` badge.
- **User Override**: If you click any other option, the selection highlight moves to represent the manual override, while the `🤖 AI Choice` badge remains on the original card.
- **Sequence Steps**: The active step count in the mood path advances with each transition.

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

### How it works with Match / Fresh / Switch

The two are layers, not alternatives:

- The **curve** owns the destination — where the set should be at step `i`.
- **Match / Fresh / Switch** owns the next step — which specific track, and how
  it is mixed in.

The four-step cycle in §3 is the *default* answer for each transition. Choosing
one by hand overrides that single step; the remaining plan is then re-searched
from the new track, against the same curve and the same step positions, so the
set still arrives where it was going. Being 60% through a Build stays 60%
through a Build — the arc is preserved, the route to it changes.
