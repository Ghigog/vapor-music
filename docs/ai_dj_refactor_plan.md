# AI DJ Engine Refactor Plan: Professional-Grade Mixing

This document outlines a comprehensive refactor plan to overhaul the AI DJ's audio analysis, playlist pathfinding, and deck transition systems. The goal is to move the codebase away from basic simulations and hardcoded approximations, bringing it in line with professional DJ standards (e.g., Rekordbox, Serato, Traktor).

---

## 1. Executive Summary & Core Architectural Flaws

The current implementation of the "AI DJ" suffers from several critical architectural limitations that prevent it from executing seamless, musical transitions. 

| System | Current Implementation | Core Defect | Impact | Status |
| :--- | :--- | :--- | :--- | :--- |
| **Audio Analysis (MP3)** | Fake byte-hash modulo mapping in [audio_analyzer.gd](file:///Users/dylangrowcoot/Documents/Personal%20Apps/vapor-music/scripts/services/audio_analyzer.gd#L474) | No actual DSP is performed; keys and BPMs are chosen randomly from static lists. | Inaccurate key/BPM calculations. Harmonic mixing is an illusion. | **To Be Scrapped** |
| **Audio Analysis (WAV)** | Naive peak envelope detection in [audio_analyzer.gd](file:///Users/dylangrowcoot/Documents/Personal%20Apps/vapor-music/scripts/services/audio_analyzer.gd#L374) | Capped at 1MB (first ~5.94 seconds of audio). No key detection (hardcoded to `8A`). | Completely misses song structure; wrong BPM if track has ambient intros. | **To Be Scrapped** |
| **Beat Synchronization** | None. Deck B plays from `0.0` instantly when triggered in [audio_manager.gd](file:///Users/dylangrowcoot/Documents/Personal%20Apps/vapor-music/scripts/services/audio_manager.gd#L660). | Lacks beat grid extraction or phase-lock loop (PLL) synchronization. | Tracks drift, kicks overlap out-of-phase, resulting in severe trainwrecks. | **To Be Scrapped** |
| **Tempo Morphing** | Tweaking `pitch_scale` in [audio_manager.gd](file:///Users/dylangrowcoot/Documents/Personal%20Apps/vapor-music/scripts/services/audio_manager.gd#L803) | Relies on pitch-shifting instead of independent time-stretching. | Gliding speeds warps pitch, creating chipmunk or demonic vocals. | **To Be Scrapped** |
| **Phrasing & Timing** | Absolute countdown timer in [audio_manager.gd](file:///Users/dylangrowcoot/Documents/Personal%20Apps/vapor-music/scripts/services/audio_manager.gd#L208) | Triggers transitions strictly based on seconds remaining. | Blends occur blindly in the middle of drops, choruses, or vocal sections. | **To Be Fixed** |
| **Transition Dynamics** | Static volume/EQ Godot Tweens in [audio_manager.gd](file:///Users/dylangrowcoot/Documents/Personal%20Apps/vapor-music/scripts/services/audio_manager.gd#L674) | Hardcoded filter sweeps and linear gain curves. | Audio clipping, muddy overlaps, no dynamic frequency-carving. | **To Be Fixed** |
| **Pathfinder** | Greedy search in [dj_pathfinder.gd](file:///Users/dylangrowcoot/Documents/Personal%20Apps/vapor-music/scripts/services/dj_pathfinder.gd#L238) | Only calculates the immediate next track's weighted cost. | Set progression has no global energy building, curve planning, or peaking. | **To Be Fixed** |

---

## 2. Refactor Blueprint: What to Scrap vs. What to Fix

### 🚨 SCRAP ENTIRELY
1.  **Faked Metadata & Module Modulo Math:** Remove the deterministic byte-hash modulo mapping inside [audio_analyzer.gd](file:///Users/dylangrowcoot/Documents/Personal%20Apps/vapor-music/scripts/services/audio_analyzer.gd#L509-L539).
2.  **6-Second WAV Cap:** Remove the 1MB file read ceiling inside [audio_analyzer.gd](file:///Users/dylangrowcoot/Documents/Personal%20Apps/vapor-music/scripts/services/audio_analyzer.gd#L399).
3.  **Godot `pitch_scale` for Tempo Morph:** Delete the lines altering `pitch_scale` to match tempo during transitions inside [audio_manager.gd](file:///Users/dylangrowcoot/Documents/Personal%20Apps/vapor-music/scripts/services/audio_manager.gd#L803-L804) and the post-transition pitch glide (lines 838-843).
4.  **Blind Incoming Deck Triggers:** Scrap the raw `incoming.play()` call that triggers playback without beat grid offset references.

### 🛠️ FIX & REWRITE
1.  **Transition Cue Triggering:** Modify `_process()` in [audio_manager.gd](file:///Users/dylangrowcoot/Documents/Personal%20Apps/vapor-music/scripts/services/audio_manager.gd#L208-L217) to evaluate phrasing boundaries (bars/beats elapsed) rather than time countdowns.
2.  **EQ/Filter Curves:** Rewrite the transition-specific curves in `_run_deck_transition()` to tie EQ bands to the crossfader progress dynamically, implementing subtractive mixing rules.
3.  **Pathfinding Algorithms:** Upgrade `generate_mood_path()` in [dj_pathfinder.gd](file:///Users/dylangrowcoot/Documents/Personal%20Apps/vapor-music/scripts/services/dj_pathfinder.gd#L238) to perform global planning over the active queue.

---

## 3. Step-by-Step Refactor Implementation Guide

### Step 1: Integrated C++ DSP Layer (GDExtension)
To achieve professional-grade audio analysis and time-stretching, Godot's GDScript layer must be augmented with a low-level C++ DSP library compiled as a **GDExtension**.

*   **Libraries Needed:**
    *   **Essentia (C++):** For extracting high-resolution tempo, downbeats, musical key, spectral centroid, and structural segments.
    *   **Rubber Band Library (C++):** A high-quality, real-time pitch-shifting and time-stretching library to adjust track speeds by ±15% with zero pitch distortion.
*   **GDExtension Interface Wrapper:** Create a C++ node wrapper `AudioDSP` exposed to GDScript:
    ```cpp
    class AudioDSP : public Node {
        GDCLASS(AudioDSP, Node);
        // Exposes time-stretching buffers and analysis functions to GDScript
    };
    ```

### Step 2: High-Fidelity Audio Analysis & Local Caching
Replace the faked analysis in [audio_analyzer.gd](file:///Users/dylangrowcoot/Documents/Personal%20Apps/vapor-music/scripts/services/audio_analyzer.gd) with a hybrid approach:

1.  **Online Metadata Pull (Web Check):**
    *   Query AcousticBrainz or Spotify/Deezer API for estimated Key, BPM, and Vibe parameters (danceability, valence, energy).
2.  **Offline Local DSP Sweep (The Truth):**
    *   Feed the *entire* audio file into the new `AudioDSP` GDExtension.
    *   Perform **onset detection** to map out a precise list of beat timestamps (`beat_grid: Array[float]`).
    *   Perform **spectral flux analysis** to identify structural boundaries (Intro, Chorus, Breakdown, Outro) and write them as timestamps.
    *   Perform **Chromagram Key Detection** to accurately resolve the musical key (major/minor Camelot scale).
3.  **Local Metadata Serialization:**
    Save the extracted profile locally inside `user://metadata_cache.json` under an expanded schema:
    ```json
    {
      "bpm": 124.03,
      "musical_key": "8A",
      "beat_grid": [0.42, 0.90, 1.38, 1.86, 2.34],
      "downbeats": [0.42, 2.34, 4.26],
      "segments": {
        "intro": [0.0, 32.0],
        "drop": [32.0, 120.0],
        "outro": [180.0, 210.0]
      }
    }
    ```

### Step 3: Beat Grid Sync & Phase-Locked Loop (PLL)
Rebuild the playback trigger in [audio_manager.gd](file:///Users/dylangrowcoot/Documents/Personal%20Apps/vapor-music/scripts/services/audio_manager.gd) to match downbeats:

1.  **Phrase Alignment:**
    Instead of playing the incoming track from `0.0`, align the beat phase:
    *   Monitor the current playback beat index of the outgoing deck.
    *   Calculate the exact sample offset required to align Beat 1 (Downbeat) of the incoming track's intro with the next bar line (e.g., Beat 17, 33, or 49) of the outgoing track.
2.  **Phase-Locked Loop:**
    *   Continuously monitor the drift between the beat timestamps of Deck A and Deck B.
    *   Apply micro-adjustments to the playback rate of the incoming deck (within ±0.5%) to keep the beat boundaries locked in phase throughout the transition.

### Step 4: True Time-Stretching & Pitch Preservation
Scrap `pitch_scale` adjustments. Hook up the **Rubber Band** engine:

1.  Calculate the target transition BPM:
    $$\text{Target BPM} = \frac{\text{BPM}_{\text{out}} + \text{BPM}_{\text{in}}}{2}$$
2.  Feed target ratios to the time-stretch engine:
    $$\text{Ratio}_{\text{out}} = \frac{\text{Target BPM}}{\text{BPM}_{\text{out}}}$$
    $$\text{Ratio}_{\text{in}} = \frac{\text{Target BPM}}{\text{BPM}_{\text{in}}}$$
3.  Process audio streams through the time-stretch buffer, ensuring that both tracks speed up or slow down to meet at the target tempo while maintaining their original musical keys.

### Step 5: Phrase-Aware Structural Transitions
Modify the transition trigger mechanism in [audio_manager.gd](file:///Users/dylangrowcoot/Documents/Personal%20Apps/vapor-music/scripts/services/audio_manager.gd#L208):

1.  **Monitor Beat Phrasing:**
    Translate the track position from seconds into **bars and beats** (e.g., Bar 64, Beat 3).
2.  **Structure-Based Cueing:**
    *   Determine the start of the Outro segment.
    *   Queue the transition to begin at the *first downbeat* of the Outro segment, provided it aligns with a standard 8-bar phrasing loop boundary.
3.  **Vocal Masking:**
    If the system detects overlapping vocal frequencies (mid-range density check) in both track profiles, delay the transition or automatically engage a wider EQ mid-band cut to prevent clashing.

### Step 6: Subtractive Dynamic EQ & Frequency-Aware Crossfading
Replace the linear Godot Tweens inside `_run_deck_transition()` with a subtractive mix model driven by a **crossfader variable** ($X \in [0.0, 1.0]$):

1.  **Bass Carving:**
    Avoid muddy low-end overlaps by crossing over the EQ gains:
    $$\text{Bass}_{\text{out}} = \text{clamp}(1.0 - 2X, 0.0, 1.0)$$
    $$\text{Bass}_{\text{in}} = \text{clamp}(2X - 1.0, 0.0, 1.0)$$
    *This formula keeps the total bass energy at 0dB, swapping the low-frequencies completely at the exact midpoint ($X = 0.5$).*
2.  **Filter Sweeps:**
    Hook the lowpass filter cutoff of the outgoing track and the highpass filter cutoff of the incoming track directly to the crossfader curves, creating a smooth spectral handoff.
3.  **Frequency RMS Monitoring:**
    Implement real-time RMS metering on the Low/Mid/High bands. If the sum energy of any band exceeds $+2\text{dB}$ relative to reference levels, automatically compress or cut that band on the outgoing deck to prevent digital clipping.

### Step 7: A* Mood Pathfinder & Genre Taxonomy Mapping
Upgrade the selection logic in [dj_pathfinder.gd](file:///Users/dylangrowcoot/Documents/Personal%20Apps/vapor-music/scripts/services/dj_pathfinder.gd):

1.  **Define Genre Map:**
    Create a static JSON taxonomy tree mapping subgenres:
    ```json
    {
      "Tech House": ["Minimal House", "Deep House", "Techno"],
      "Liquid DNB": ["Jungle", "Neurofunk", "Ambient House"]
    }
    ```
2.  **A* Global Set Planning:**
    Rewrite `generate_mood_path()` to search the library using A* pathfinding.
    *   **Cost Function:** Add penalty weights for sudden genre jumps, Camelot key leaps, and huge energy swings.
    *   **Energy Target Curve:** Allow the user to select an overall playlist arc (e.g., "Build Vibe", "Chill Down", or "Wave"). The pathfinder will select tracks that match the target energy slope over a 10-song sequence.

---

## 4. API & Code Symbol Contracts for the Future Agent

The following script targets must be updated to support the new architecture:

### 📄 `scripts/services/audio_analyzer.gd`
```gdscript
# NEW API CONTRACT
func analyze_track(href: String, priority: bool = false) -> void:
    # 1. Check metadata cache.
    # 2. If missing, query Web API for metadata.
    # 3. Stream audio to local disk, run AudioDSP C++ GDExtension.
    # 4. Extract beat_grid, downbeats, key, and segments.
    # 5. Save results to metadata_cache.json.
```

### 📄 `scripts/services/audio_manager.gd`
```gdscript
# NEW API CONTRACT
func start_transition(force_immediate: bool = false) -> void:
    # 1. Retrieve beat_grid, downbeats, and segments for outgoing and incoming.
    # 2. Calculate beat alignment offset & phase lock ratio.
    # 3. Initialize Rubber Band time-stretcher on both decks.
    # 4. Schedule incoming player start on exact downbeat phase.
    # 5. Bind EQ gains and filters to the crossfader tween.
```

### 📄 `scripts/services/dj_pathfinder.gd`
```gdscript
# NEW API CONTRACT
static func generate_mood_path(tracks_meta: Dictionary, start_href: String = "", target_curve: String = "build") -> Array[String]:
    # 1. Construct graph nodes from tracks_meta.
    # 2. Build cost weights based on Camelot key distance, BPM ratio, and genre tree.
    # 3. Run global A* search matching the target_curve energy profile.
    # 4. Return sorted playlist array.
```
