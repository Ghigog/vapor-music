//! Vibe DJ commands — choosing what comes next, and how it gets there.

use tauri::State;

// These modules are `lib.rs` split up, not new boundaries — the glob is
// what says so. Narrowing it to forty named symbols would read as a design.
use crate::*;

/// Choose the energy curve the set is conducted along.
///
/// Selecting one *is* the action. There is no "conduct from here" button any
/// more: a curve that has been chosen and not applied is a control that lies
/// about what it did.
///
/// The tail is dropped and re-planned, because the curve is the set's
/// destination and the tracks queued behind it were routes to a different one.
/// What is playing, and what is mixing into it right now, are left alone.
#[tauri::command]
pub fn set_curve(
    curve: String,
    app_handle: tauri::AppHandle,
    state: State<'_, Shared>,
) -> Result<Settings> {
    let (settings, generation, plan) = {
        let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
        app.settings.curve = vapor_library::Curve::parse(&curve).as_str().to_string();
        app.save_settings()?;
        app.curve_plan = app.curve_plan.wrapping_add(1);

        // Everything after the track playing is a route to the old destination.
        let keep = app.queue.current_index().map(|i| i + 1).unwrap_or(0);
        let head: Vec<String> = app.queue.tracks().iter().take(keep).cloned().collect();
        if !head.is_empty() {
            let playing = app.playing.clone();
            app.queue.set_tracks(head, playing.as_deref());
        }

        // Gathered here, searched elsewhere. Building the pool needs the state;
        // walking it does not.
        // Built once and used twice. This runs under the state lock on a press
        // that already has history for being slow, and walking the whole
        // library to answer one lookup and then walking it again to plan the
        // route would be paying that cost twice for one press.
        let pool = track_meta_pool(&app);
        // Where this route starts from, kept so the curve can be evaluated
        // later — see `AppState::curve_start` and `AppState::curve_origin`.
        // The curve is measured from *this* press, so the shape is continuous
        // however many times the tail is topped up afterwards.
        app.curve_span = vapor_library::Span::of(&pool);
        if let Some(playing) = app.playing.clone() {
            app.curve_start = pool.get(&playing).map_or(0.5, TrackMeta::curve_energy);
            app.curve_origin = Some(playing);
        }
        let plan = app.playing.is_some() && app.settings.dj_mode;
        (app.settings.clone(), app.curve_plan, plan)
    };

    // The set is refilled off the command thread, and off the lock.
    //
    // It used to run right here: an A* over the whole library for ten tracks,
    // seconds of work, with the state lock held for all of it. So the press did
    // not return until the set had been re-planned — the button appeared dead
    // for five seconds and then caught up — and every poll that wanted the same
    // lock stalled behind it, which is why the screen froze rather than just the
    // control.
    //
    // Now it is ten separate decisions, each taking and releasing the lock, each
    // announced. The screen sees the tail empty and then refill a track at a
    // time, which is what it should look like: a DJ deciding, not a batch job
    // finishing.
    if plan {
        let shared: Shared = Arc::clone(&state);
        tauri::async_runtime::spawn_blocking(move || {
            refill_set(&shared, &app_handle, generation);
        });
    }

    Ok(settings)
}

/// Top the set back up to [`PLAN_AHEAD`], one track at a time, announcing each.
///
/// `generation` is the value of `AppState::curve_plan` this refill was started
/// for; a newer press bumps it and this stands down mid-way rather than filling
/// a queue that is already heading somewhere else.
///
/// The lock is taken and released once per track on purpose. Holding it for the
/// whole refill would be the old freeze in a new shape — the screen polls
/// `playback_state` and `queue_view` on a timer and both want this lock, so a
/// refill that holds it for a second is a second of frozen UI.
fn refill_set(shared: &Shared, app_handle: &tauri::AppHandle, generation: u64) {
    use tauri::Emitter as _;
    for _ in 0..PLAN_AHEAD {
        let Ok(mut app) = shared.lock() else {
            return;
        };
        // A newer press is already on its way somewhere else.
        if app.curve_plan != generation {
            return;
        }
        let grew = extend_set(&mut app);
        drop(app);
        if !grew {
            return;
        }
        let _ = app_handle.emit("playback-changed", ());
    }
}

/// Turn the DJ on or off.
///
/// Persisted, because it decides what the *supervisor* does — it lived in the
/// frontend as component state until 2026-08-17, which meant the half of the
/// app that chooses what plays next had never heard of it.
#[tauri::command]
pub fn set_dj_mode(enabled: bool, state: State<'_, Shared>) -> Result<Settings> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    app.settings.dj_mode = enabled;
    app.save_settings()?;
    Ok(app.settings.clone())
}

/// Set the Vibe Limit — §6's Mix Tuner.
///
/// Refused rather than clamped when it is not a number at all: a slider cannot
/// produce one, so a NaN here means something else is wrong and quietly
/// substituting a value would hide it. Out-of-range *is* clamped, because the
/// ends of the slider are exactly the ends of the band.
#[tauri::command]
pub fn set_vibe_limit(limit: f32, state: State<'_, Shared>) -> Result<Settings> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    if !limit.is_finite() {
        return Err(Error("That is not a Vibe Limit.".to_string()));
    }
    app.settings.vibe_limit = limit.clamp(
        vapor_library::settings::MIN_VIBE_LIMIT,
        vapor_library::settings::MAX_VIBE_LIMIT,
    );
    app.save_settings()?;
    Ok(app.settings.clone())
}

/// Order a set of tracks along an energy and tempo curve.
///
/// The BPM overrides in settings are applied before pathfinding rather than
/// after: tempo detection lands a metrical relative on roughly 10% of a real
/// library, and a wrong BPM does not merely mislabel a track — it changes
/// which transitions the pathfinder believes are cheap, so a correction has to
/// reach the cost model to be worth anything.
#[tauri::command]
pub fn mood_path(req: MoodPathRequest, state: State<'_, Shared>) -> Result<Vec<String>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;

    let mut tracks = req.tracks;
    for (href, meta) in tracks.iter_mut() {
        // A hand correction, or the genre's verdict on which octave the
        // detector read (AUD-26). The caller sent these metas in, and its copy
        // predates both.
        if let Some(bpm) = crate::tempo_in_force(&app, href, app.analysis.get(href)) {
            meta.bpm = bpm;
        }
    }

    Ok(vapor_library::generate_mood_path(
        &tracks,
        &req.start,
        Curve::parse(&req.curve),
        app.settings.vibe_limit,
        &skip_penalties(&app),
    ))
}

/// Order a set of tracks along an energy and tempo curve, from what is known.
#[tauri::command]
pub fn vibe_path(start: String, curve: String, state: State<'_, Shared>) -> Result<VibePath> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;
    let pool = track_meta_pool(&app);

    if !pool.contains_key(&start) {
        return Err(Error(
            "That track has not been analysed yet, so the DJ has nothing to plan from.".to_string(),
        ));
    }

    let considered = pool.len();
    let skipped = app.rows.len().saturating_sub(considered);
    Ok(VibePath {
        hrefs: vapor_library::generate_mood_path(
            &pool,
            &start,
            Curve::parse(&curve),
            app.settings.vibe_limit,
            // What the app has learned from being skipped (TD-14).
            &skip_penalties(&app),
        ),
        considered,
        skipped,
    })
}

/// The three ways out of the playing track, and which one the DJ would take.
///
/// §2–§4 of `docs/ai_dj_workflow.md`. The curve decides where the set is going;
/// this decides the next step. Both existed in the original — `play_harmonic_
/// shuffle` planned the arc and this chose each transition — and the rewrite
/// shipped only the first, so the screen could plan a set but never show the
/// choice it was making or let anyone overrule it.
///
/// One candidate per kind, each the best of its kind rather than the best
/// overall, because the point is to offer three genuinely different exits.
#[tauri::command]
pub fn mix_candidates(state: State<'_, Shared>) -> Result<Vec<MixCandidate>> {
    let mut app = state.lock().map_err(|e| Error(e.to_string()))?;
    Ok(mix_candidates_for(&mut app))
}

#[tauri::command]
pub fn choose_next(
    href: String,
    curve: String,
    app_handle: tauri::AppHandle,
    state: State<'_, Shared>,
) -> Result<()> {
    let generation = {
        let mut app = state.lock().map_err(|e| Error(e.to_string()))?;

        if !app.queue.set_next(&href) {
            return Err(Error("That track is no longer in the queue.".to_string()));
        }

        // The queue is not the whole story: a mix armed for a different track
        // has that track loaded on the deck already, and the queue does not
        // reach it. Drop it so the supervisor arms the chosen one instead.
        let running = app.player.as_ref().is_some_and(|p| p.transition_running());
        if mix_must_be_rearmed(app.armed_next.as_deref(), &href, running) {
            if let Some(player) = app.player.as_ref() {
                player.cancel_transition();
            }
            // Cleared as well as cancelled: an `arm_mix` still in flight checks
            // this before handing its decoder over, so clearing it is what
            // makes that one stand down too.
            app.armed_next = None;
        }

        app.settings.curve = vapor_library::Curve::parse(&curve).as_str().to_string();
        app.curve_plan = app.curve_plan.wrapping_add(1);

        // Everything after the chosen track was a route somewhere else.
        let chosen = app
            .queue
            .tracks()
            .iter()
            .position(|t| *t == href)
            .map(|i| i + 1)
            .unwrap_or(0);
        if chosen > 0 {
            let head: Vec<String> = app.queue.tracks().iter().take(chosen).cloned().collect();
            let playing = app.playing.clone();
            app.queue.set_tracks(head, playing.as_deref());
        }

        let pool = track_meta_pool(&app);
        // The tail is re-planned from the chosen track, so that track is what
        // the curve is now relative to.
        app.curve_span = vapor_library::Span::of(&pool);
        if let Some(t) = pool.get(&href) {
            app.curve_start = t.curve_energy();
            app.curve_origin = Some(href.clone());
        }
        app.curve_plan
    };

    // The exit takes effect above; the route behind it is filled in off the
    // lock, a track at a time. Same reason as `set_curve`: the queue's next
    // track is already set by the time this returns, which is the part the
    // press was actually asking for.
    let shared: Shared = Arc::clone(&state);
    let handle = app_handle.clone();
    tauri::async_runtime::spawn_blocking(move || {
        refill_set(&shared, &handle, generation);
    });
    let _ = tauri::Emitter::emit(&app_handle, "playback-changed", ());
    Ok(())
}

/// Describe the mix between what is playing and what is next.
///
/// Read-only: this asks the same questions `plan_mix` asks and answers them for
/// a person instead of for the audio thread, so the screen cannot claim a blend
/// the engine would refuse.
#[tauri::command]
pub fn blend_preview(state: State<'_, Shared>) -> Result<Option<BlendPreview>> {
    let app = state.lock().map_err(|e| Error(e.to_string()))?;

    let Some(current) = app.playing.clone() else {
        return Ok(None);
    };
    let Some(next) = app.queue.peek_next(None).map(str::to_string) else {
        return Ok(None);
    };
    if next == current {
        return Ok(None);
    }

    let title_of = |href: &str| {
        app.rows
            .iter()
            .find(|r| r.href == href)
            .map(|r| r.title.clone())
            .unwrap_or_default()
    };

    let (Some(out), Some(inc)) = (app.analysis.get(&current), app.analysis.get(&next)) else {
        return Ok(Some(BlendPreview {
            from_title: title_of(&current),
            to_title: title_of(&next),
            from_bpm: 0.0,
            to_bpm: 0.0,
            from_key: String::new(),
            to_key: String::new(),
            shift_percent: 0.0,
            gain_delta: 0.0,
            matchable: false,
            reason: "Not analysed yet".to_string(),
            transition: "crossfade".to_string(),
        }));
    };

    // The tempo the mix will actually be built at, so the number this screen
    // reports and the number the stretcher meets cannot disagree (AUD-26).
    let from_tempo = crate::tempo_in_force(&app, &current, Some(out));
    let to_tempo = crate::tempo_in_force(&app, &next, Some(inc));
    let from_bpm = from_tempo.unwrap_or(out.bpm);
    let to_bpm = to_tempo.unwrap_or(inc.bpm);

    let out_grid = beat_grid(out, from_tempo);
    let in_grid = beat_grid(inc, to_tempo);
    let matched = vapor_engine::Mixer::tempo_ratio(&out_grid, &in_grid);

    let (matchable, reason, shift_percent) = match matched {
        Ok(ratio) => (true, String::new(), ((ratio - 1.0) * 100.0) as f32),
        Err(vapor_engine::MatchError::TempoTooFar) => (
            false,
            "Too far apart to beat-match".to_string(),
            if to_bpm > 0.0 {
                (from_bpm / to_bpm - 1.0) * 100.0
            } else {
                0.0
            },
        ),
        Err(vapor_engine::MatchError::NoGrid) => (false, "No usable beat grid".to_string(), 0.0),
    };

    Ok(Some(BlendPreview {
        from_title: title_of(&current),
        to_title: title_of(&next),
        from_bpm,
        to_bpm,
        from_key: out.key.clone(),
        to_key: inc.key.clone(),
        shift_percent,
        gain_delta: inc.lufs - out.lufs,
        matchable,
        reason,
        transition: transition_name(vapor_dj::choose_transition(
            if out.outro_key.is_empty() {
                &out.key
            } else {
                &out.outro_key
            },
            if inc.intro_key.is_empty() {
                &inc.key
            } else {
                &inc.intro_key
            },
            (from_bpm - to_bpm).abs(),
            same_genre(&app, &current, &next),
        )),
    }))
}
