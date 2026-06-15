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
The AI DJ automatically selects from 6 transition types based on the BPM difference and the key relationship (harmonic, modulated, or clashing) to mimic how a real DJ plays. Transition durations are phrase-adaptive: if outro/intro segment metadata is available for both tracks, the duration is dynamically set as the minimum of the two segment lengths, clamped between `3.0s` and `16.0s`. Otherwise, it falls back to the transition type's default duration.

Transition loading is triggered `duration + 4.0` seconds before the track ends, and begins exactly at `duration` seconds remaining:

- **Bass Swap** (BPM diff < 3.0, Harmonic or Modulated): 6.0s blend. Low EQ frequencies crossfade smoothly around the midpoint to prevent abrupt energy cuts.
- **Filter Sweep** (BPM diff 3.0–8.0, Harmonic): 4.0s blend. Outgoing lowpass and incoming highpass sweeps.
- **Tempo Morph** (BPM diff 3.0–8.0, Modulated/Clashing): 6.0s blend. Syncs tempos during crossfade, then ramps to native tempo.
- **Reverb Freeze** (BPM diff < 8.0, Clashing / Switch): 5.0s blend. Outgoing reverb freezes at midpoint to wash out the clashing frequencies and mask the transition.
- **Echo Out** (BPM diff >= 8.0, Clashing/Modulated or Fresh/Switch): 5.0s blend. Outgoing delay rings out from midpoint to mask key clashes and major BPM jumps.
- **Standard Crossfade** (BPM diff >= 8.0, Harmonic): 3.0s blend. Fast linear volume crossfade.
