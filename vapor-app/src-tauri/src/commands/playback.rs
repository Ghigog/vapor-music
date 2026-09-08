//! Playback commands — transport, volume, repeat and shuffle.

use tauri::State;

// These modules are `lib.rs` split up, not new boundaries — the glob is
// what says so. Narrowing it to forty named symbols would read as a design.
use crate::*;

/// Whether hardware media keys will reach this build (MIG-023).
///
/// False on macOS when the process is not inside a `.app` bundle: the keys go
/// to the Now Playing *application*, and a bare binary from `tauri dev` is not
/// one. Reported rather than left to be discovered, because souvlaki registers
/// successfully either way and the only other symptom is a keyboard that does
/// nothing.
#[tauri::command]
pub fn media_keys_available() -> bool {
    media::bundled()
}

#[tauri::command]
pub fn play_tracks(
    app_handle: tauri::AppHandle,
    hrefs: Vec<String>,
    start: Option<String>,
    scope: Option<String>,
    collection: Option<CollectionRef>,
    state: State<'_, Shared>,
) -> Result<()> {
    let shared: Shared = Arc::clone(&state);

    let (jump_the_queue, conduct) = {
        let mut app = shared.lock().map_err(|e| Error(e.to_string()))?;
        // A named scope confines the set to what was played from; no name is
        // the library, which is what an unfiltered list means.
        app.scope = scope.filter(|n| !n.trim().is_empty()).map(|name| Scope {
            name,
            tracks: hrefs.iter().cloned().collect(),
        });
        // Which shelf this was put on from, if it was one. Set before the
        // queue moves, because `begin_playback` reads it to decide what the
        // listen it is about to earn should be credited to.
        //
        // Assigned unconditionally, `None` included: playing an album after a
        // playlist has to stop crediting the playlist, and only overwriting
        // when a collection is named would leave the previous one attached to
        // everything played afterwards.
        app.collection = collection.as_ref().and_then(collection_key);

        // Re-entering the queue, or starting a new set?
        //
        // The Queue screen's "play from here" hands back the queue it is
        // already showing, because jumping down a list is not a change of what
        // you are listening to — so the order somebody dragged into place has
        // to survive it. Every other caller hands a *list*: an album, a
        // playlist, a dynamic group, a search result. Compared rather than
        // flagged by the caller so no screen can forget to say which it is.
        let re_entering = hrefs == app.queue.tracks();

        app.queue.set_tracks(hrefs, start.as_deref());
        let current = app.queue.current().map(str::to_string);

        // With the DJ on, a list is a *pool*, not a running order.
        //
        // This is the half of "conducting" that was missing. `play_tracks` put
        // the whole list in the queue in list order, and `extend_set` only ever
        // plans past the end of what is queued — so pressing play on a group
        // with the DJ on gave you that group alphabetically, by artist, while
        // the screen said "conducted by Vibe". Nothing was conducted until the
        // list ran out, which for a real library is never. Pressing a curve was
        // the only thing that ever re-planned, which is why the curves appeared
        // to work and simply starting a set did not.
        //
        // So the tail is dropped and the DJ plans it, exactly as choosing a
        // curve does. `app.scope` is the list, so the route stays inside it.
        if app.settings.dj_mode && !re_entering {
            if let Some(current) = current.clone() {
                app.queue.set_tracks(vec![current.clone()], Some(&current));
                app.curve_plan = app.curve_plan.wrapping_add(1);
                // A new set is a new curve, measured from here — otherwise the
                // shape would carry on from wherever the last one had got to.
                let pool = track_meta_pool(&app);
                app.curve_span = vapor_library::Span::of(&pool);
                app.curve_start = pool.get(&current).map_or(0.5, TrackMeta::curve_energy);
                app.curve_origin = Some(current);
                // A different pool entirely, so whatever it last found nothing
                // in is not evidence about this one.
                app.set_exhausted = None;
            }
        }

        if let Some(current) = current.clone() {
            begin_playback(&shared, &mut app, current);
        }
        let conduct = app.settings.dj_mode.then_some(app.curve_plan);
        // The track being started, and the few behind it.
        //
        // This used to ask only about the current track, which meant a queue
        // whose *next* records were undescribed never re-ordered the pass — so
        // the DJ reached a track it knew nothing about and could not mix into
        // it. `plan_mix` needs the incoming track analysed before it arrives,
        // not while it is arriving.
        //
        // Still bounded, for the reason the old comment gave: restarting costs
        // the track in flight, and paying that to re-order a queue that is
        // already described would be worse than leaving the pass alone.
        (needs_analysis_soon(&app, MIX_LOOKAHEAD), conduct)
    };

    // The set is filled in off the lock, a track at a time, each one
    // announced — the same path a curve press takes, so a set that is starting
    // and a set that is being re-aimed look the same on screen.
    if let Some(generation) = conduct {
        let planning: Shared = Arc::clone(&state);
        let handle = app_handle.clone();
        tauri::async_runtime::spawn_blocking(move || {
            crate::commands::dj::refill_set(&planning, &handle, generation);
        });
    }

    if jump_the_queue {
        // Lock released above: `start_analysis` takes it.
        start_analysis(&app_handle, &shared)?;
    }
    Ok(())
}

#[tauri::command]
pub fn next_track(state: State<'_, Shared>) -> Result<Option<String>> {
    let shared: Shared = Arc::clone(&state);
    let mut app = shared.lock().map_err(|e| Error(e.to_string()))?;

    record_skip_if_reacting_to_a_blend(&mut app);

    let next = app.queue.next(None).map(str::to_string);
    if let Some(href) = next.clone() {
        begin_playback(&shared, &mut app, href);
    }
    Ok(next)
}

#[tauri::command]
pub fn previous_track(state: State<'_, Shared>) -> Result<Option<String>> {
    let shared: Shared = Arc::clone(&state);
    let mut app = shared.lock().map_err(|e| Error(e.to_string()))?;
    let previous = app.queue.previous().map(str::to_string);
    if let Some(href) = previous.clone() {
        begin_playback(&shared, &mut app, href);
    }
    Ok(previous)
}

#[tauri::command]
pub fn playback_state(state: State<'_, Shared>) -> Result<PlaybackState> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;

    let snapshot = app.player.as_ref().map(|p| p.snapshot());
    let row = app
        .playing
        .as_ref()
        .and_then(|href| app.rows.iter().find(|r| &r.href == href));
    let next = app
        .queue
        .peek_next(None)
        .and_then(|href| app.rows.iter().find(|r| r.href == href));
    let analysis = app.playing.as_ref().and_then(|href| app.analysis.get(href));

    let beat = match (analysis, snapshot.as_ref()) {
        (Some(a), Some(s)) => beat_window(
            a,
            s.position,
            // The tempo in force, not the raw reading: the ribbon pulses on
            // this, and a pulse at half the tempo the mixer is using is the
            // visible half of AUD-26.
            crate::tempo_in_force(&app, app.playing.as_deref().unwrap_or_default(), Some(a))
                .unwrap_or(a.bpm),
        ),
        _ => (0.0, 0.0),
    };

    /*
     * Where the set is, along the curve it was planned on.
     *
     * Read off the queue rather than kept as its own list: the queue IS the
     * plan once `generate_mood_path` has appended to it, and a second copy
     * would be a second thing to keep in step. Off in shuffle, where there is
     * no curve and a position along one would be fiction.
     */
    let set = if app.settings.dj_mode {
        let total = app.queue.tracks().len();
        let index = app.queue.current_index().unwrap_or(0);
        if total > 1 {
            (
                index as u32,
                total as u32,
                // How far along the curve the set actually is, counted from
                // where the curve was chosen rather than divided into the
                // queue's length. A set has no end now — the queue is topped up
                // a track at a time and never runs out — so "index of total"
                // was a fraction of a number that only ever grows, and the mark
                // read as barely moving however long the set ran.
                vapor_library::Curve::parse(&app.settings.curve).target_energy(
                    app.curve_start,
                    crate::curve_step_at(&app, index),
                    &app.curve_span,
                ),
            )
        } else {
            (0, 0, app.curve_start)
        }
    } else {
        (0, 0, 0.0)
    };

    Ok(PlaybackState {
        href: app.playing.clone(),
        title: row.map(|r| r.title.clone()).unwrap_or_default(),
        // The same rule the table follows: unknown renders as a dash, never as
        // a guess.
        artist: row
            .filter(|r| r.artist_source != vapor_library::index::Source::Unknown)
            .map(|r| r.artist.clone())
            .unwrap_or_default(),
        genre: row
            .map(|r| crate::shown_genre_for_row(&app, r))
            .unwrap_or_default(),
        status: snapshot.map_or(audio::Status::Idle, |s| s.status),
        loading: app.loading,
        position: snapshot.map_or(0.0, |s| s.position),
        duration: playing_duration(snapshot.as_ref(), analysis),
        volume: snapshot.map_or(1.0, |s| s.volume),
        error: app.playback_error.clone(),
        available: app.player.is_some(),
        mixing: app.player.as_ref().is_some_and(|p| p.transition_armed()),
        level: snapshot.map_or(0.0, |s| s.level),
        brightness: snapshot.map_or(0.0, |s| s.brightness),
        beat_period: beat.0,
        next_beat: beat.1,
        set_index: set.0,
        set_total: set.1,
        set_energy: set.2,
        waveform: analysis.map(|a| a.waveform.clone()).unwrap_or_default(),
        next_title: next.map(|r| r.title.clone()).unwrap_or_default(),
        next_artist: next
            .filter(|r| r.artist_source != vapor_library::index::Source::Unknown)
            .map(|r| r.artist.clone())
            .unwrap_or_default(),
        next_album: next
            .filter(|r| r.album_source.is_known())
            .map(|r| r.album.clone())
            .unwrap_or_default(),
        next_href: next.map(|r| r.href.clone()).unwrap_or_default(),
        cover: app.playing.as_deref().and_then(|href| app.covers.get(href)),
        scope: app
            .scope
            .as_ref()
            .map(|s| s.name.clone())
            .unwrap_or_default(),
    })
}

#[tauri::command]
pub fn pause_playback(state: State<'_, Shared>) -> Result<()> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    player(&app)?.pause();
    Ok(())
}

#[tauri::command]
pub fn resume_playback(state: State<'_, Shared>) -> Result<()> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    player(&app)?.play();
    Ok(())
}

/// Stop and forget what was playing.
///
/// Deliberately not a pause: the position returns to the start and the
/// transport reads as idle, which is what the Godot build's stop button did.
#[tauri::command]
pub fn stop_playback(state: State<'_, Shared>) -> Result<()> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    player(&app)?.stop();
    // Any load still in flight now belongs to nothing, so retire its
    // generation rather than let it start playing after a stop.
    app.generation += 1;
    app.loading = false;
    app.playing = None;
    Ok(())
}

#[tauri::command]
pub fn seek(seconds: f64, state: State<'_, Shared>) -> Result<()> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    player(&app)?.seek(seconds);
    Ok(())
}

#[tauri::command]
pub fn set_volume(volume: f32, state: State<'_, Shared>) -> Result<()> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    player(&app)?.set_volume(volume);
    Ok(())
}

#[tauri::command]
pub fn set_repeat(mode: String, state: State<'_, Shared>) -> Result<()> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    app.queue.set_repeat(match mode.as_str() {
        "off" => vapor_library::Repeat::Off,
        "one" => vapor_library::Repeat::One,
        _ => vapor_library::Repeat::All,
    });
    Ok(())
}

/// Shuffle the queue, or put it back.
///
/// The permutation is generated here rather than in the core, which owns no
/// randomness on purpose — `randi()` inside a library is what made the
/// GDScript's mood paths reshuffle for no reason.
#[tauri::command]
pub fn set_shuffled(shuffled: bool, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    if !shuffled {
        return Ok(app.queue.unshuffle());
    }

    let n = app.queue.tracks().len();
    if n < 2 {
        return Ok(false);
    }
    let mut order: Vec<usize> = (0..n).collect();
    // Fisher-Yates over a cheap PRNG. Nothing here is security-sensitive and
    // pulling in `rand` for one shuffle is not worth the dependency.
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x2545F4914F6CDD1D)
        | 1;
    for i in (1..n).rev() {
        // xorshift64*
        seed ^= seed >> 12;
        seed ^= seed << 25;
        seed ^= seed >> 27;
        let j = (seed.wrapping_mul(0x2545F4914F6CDD1D) >> 33) as usize % (i + 1);
        order.swap(i, j);
    }
    Ok(app.queue.shuffle(&order))
}

/// Put a track next without disturbing the rest of the order.
#[tauri::command]
pub fn play_next(href: String, state: State<'_, Shared>) -> Result<bool> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(app.queue.set_next(&href))
}
