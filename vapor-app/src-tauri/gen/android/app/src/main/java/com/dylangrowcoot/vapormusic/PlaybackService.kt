package com.dylangrowcoot.vapormusic

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.PowerManager
import android.support.v4.media.MediaMetadataCompat
import android.support.v4.media.session.MediaSessionCompat
import android.support.v4.media.session.PlaybackStateCompat
import androidx.core.app.NotificationCompat
import androidx.media.app.NotificationCompat.MediaStyle
import androidx.media.session.MediaButtonReceiver

/**
 * Keeps the process alive while something is playing, and puts the transport in
 * the notification shade.
 *
 * ## Why it exists
 *
 * Android suspends a backgrounded process. The audio runs on its own thread in
 * Rust, and that thread is suspended with everything else: the deck starves,
 * which is heard as stuttering, and then the process is frozen and the music
 * stops. Turning the screen off does it, and so does opening another app. The
 * analysis pass dies the same way.
 *
 * A foreground service is the only thing that changes that, and a foreground
 * service must post a notification — which is the media notification anyway, so
 * the lock screen controls, the headset buttons and Bluetooth come with it
 * rather than being a second job.
 *
 * ## What it does not do
 *
 * It plays nothing. The decks, the mixer and the queue are in Rust and stay
 * there; this owns a `MediaSessionCompat` for metadata and buttons, and a wake
 * lock. Everything it learns arrives through [update]; everything a person
 * presses leaves through [onMediaButton], which is implemented in
 * `src/android.rs`.
 */
class PlaybackService : android.app.Service() {

    private var session: MediaSessionCompat? = null
    private var wakeLock: PowerManager.WakeLock? = null

    /*
     * Two reasons to be alive, not one.
     *
     * The service began as the thing that keeps *playback* going, and analysis
     * — which is a long download-bound job the app also has to survive being
     * backgrounded for — had no protection at all: switch to another app with
     * nothing playing and Android froze the process mid-pass.
     *
     * So the service stays up while either is true and goes away when neither
     * is. The notification prefers playback, because that is the one a person
     * is listening to; analysis takes it over when nothing is.
     */
    private var playing = false
    private var title = ""
    private var artist = ""
    private var position = 0L
    private var duration = 0L

    private var analysing = false
    private var analysedDone = 0
    private var analysedTotal = 0

    override fun onBind(intent: Intent?) = null

    override fun onCreate() {
        super.onCreate()

        val channel = NotificationChannel(
            CHANNEL,
            "Playback",
            // Low: this is a status readout, not an event. IMPORTANCE_DEFAULT
            // would make every track change chime.
            NotificationManager.IMPORTANCE_LOW,
        ).apply {
            description = "What Vapor is playing"
            setShowBadge(false)
        }
        getSystemService(NotificationManager::class.java).createNotificationChannel(channel)

        session = MediaSessionCompat(this, "VaporMusic").apply {
            setCallback(object : MediaSessionCompat.Callback() {
                override fun onPlay() = onMediaButton(PRESS_PLAY)
                override fun onPause() = onMediaButton(PRESS_PAUSE)
                override fun onSkipToNext() = onMediaButton(PRESS_NEXT)
                override fun onSkipToPrevious() = onMediaButton(PRESS_PREVIOUS)
                override fun onStop() = onMediaButton(PRESS_PAUSE)
            })
            isActive = true
        }

        // Partial: the CPU stays awake, the screen does not. Released in
        // `onDestroy`, and the service is stopped as soon as nothing is
        // playing, so this is not held across an idle app.
        wakeLock = getSystemService(PowerManager::class.java)
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "vapor:playback")
            .apply { setReferenceCounted(false) }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // Buttons on the notification arrive as intents rather than through the
        // callback, and this is what turns them back into session events.
        session?.let { MediaButtonReceiver.handleIntent(it, intent) }

        when (intent?.getStringExtra(EXTRA_KIND)) {
            KIND_STOP_ANALYSIS -> {
                // The Stop button on the notification. The pass is told to wind
                // down through the same path the in-app button uses; this only
                // stops being a reason for the service to exist.
                analysing = false
                onStopAnalysis()
            }
            KIND_ANALYSIS -> {
                analysing = intent.getBooleanExtra(EXTRA_ACTIVE, false)
                analysedDone = intent.getIntExtra(EXTRA_DONE, 0)
                analysedTotal = intent.getIntExtra(EXTRA_TOTAL, 0)
            }
            else -> {
                title = intent?.getStringExtra(EXTRA_TITLE) ?: ""
                artist = intent?.getStringExtra(EXTRA_ARTIST) ?: ""
                playing = intent?.getBooleanExtra(EXTRA_PLAYING, false) ?: false
                position = intent?.getLongExtra(EXTRA_POSITION, 0L) ?: 0L
                duration = intent?.getLongExtra(EXTRA_DURATION, 0L) ?: 0L
            }
        }

        // Nothing left to stay up for.
        if (!playing && title.isEmpty() && !analysing) {
            /*
             * Answer the promise before going, even though we are going.
             *
             * `startForegroundService` is a promise that `startForeground`
             * follows within a few seconds, and Android holds the app to it
             * whatever happens in between: stopping without one is
             * `ForegroundServiceDidNotStartInTimeException`, which is not an
             * exception the app can catch — the framework posts it to the main
             * thread and the process dies. That is what killed every release
             * APK from v2.0.0-rc.3 to rc.7 on launch, and none of it was R8.
             *
             * The notification goes up and comes down in the same breath, so
             * nothing is drawn. It is the documented way to answer a start you
             * no longer want.
             */
            enterForeground(notification())
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
            return START_NOT_STICKY
        }

        publish()

        // The CPU has to keep running for either of them — a frozen process
        // does not decode audio and does not finish a download.
        if (playing || analysing) {
            if (wakeLock?.isHeld == false) wakeLock?.acquire(WAKE_LOCK_LIMIT)
        } else if (wakeLock?.isHeld == true) {
            wakeLock?.release()
        }

        // Sticky would have Android restart this with a null intent after a
        // kill, which would put up a notification for a track that is not
        // playing. There is nothing to resume without the app.
        return START_NOT_STICKY
    }

    private fun publish() {
        // Null-safe rather than an early return: whatever the session is doing,
        // this call owes Android a `startForeground` and has to reach the end.
        session?.setMetadata(
            MediaMetadataCompat.Builder()
                .putString(MediaMetadataCompat.METADATA_KEY_TITLE, title)
                .putString(MediaMetadataCompat.METADATA_KEY_ARTIST, artist)
                .putLong(MediaMetadataCompat.METADATA_KEY_DURATION, duration)
                .build(),
        )

        session?.setPlaybackState(
            PlaybackStateCompat.Builder()
                .setActions(
                    PlaybackStateCompat.ACTION_PLAY or
                        PlaybackStateCompat.ACTION_PAUSE or
                        PlaybackStateCompat.ACTION_PLAY_PAUSE or
                        PlaybackStateCompat.ACTION_SKIP_TO_NEXT or
                        PlaybackStateCompat.ACTION_SKIP_TO_PREVIOUS,
                )
                .setState(
                    if (playing) PlaybackStateCompat.STATE_PLAYING
                    else PlaybackStateCompat.STATE_PAUSED,
                    position,
                    // The shade advances the position itself from this rate, so
                    // a paused notification does not keep counting.
                    if (playing) 1.0f else 0.0f,
                )
                .build(),
        )

        enterForeground(notification())
    }

    /**
     * The notification this service is required to have, for whichever of its
     * two reasons is current.
     *
     * Separate from [publish] because [onStartCommand] needs one on the way out
     * as well, where there is no session state left to publish.
     */
    private fun notification(): Notification {
        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        // Playback wins the notification when there is any; otherwise it
        // reports the pass, so a person who backgrounded the app during one can
        // see it is still going and how far it has got.
        val showingPlayback = title.isNotEmpty()
        val heading = if (showingPlayback) title else "Listening to your library"
        val body =
            if (showingPlayback) artist
            else "$analysedDone of $analysedTotal analysed"

        val builder = NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(android.R.drawable.ic_media_play)
            .setContentTitle(heading)
            .setContentText(body)
            .setContentIntent(open)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)

        if (showingPlayback) {
            builder
                .addAction(
                    android.R.drawable.ic_media_previous,
                    "Previous",
                    action(PlaybackStateCompat.ACTION_SKIP_TO_PREVIOUS),
                )
                .addAction(
                    if (playing) android.R.drawable.ic_media_pause
                    else android.R.drawable.ic_media_play,
                    if (playing) "Pause" else "Play",
                    action(PlaybackStateCompat.ACTION_PLAY_PAUSE),
                )
                .addAction(
                    android.R.drawable.ic_media_next,
                    "Next",
                    action(PlaybackStateCompat.ACTION_SKIP_TO_NEXT),
                )
                .setStyle(
                    MediaStyle()
                        .setMediaSession(session?.sessionToken)
                        // Which of the three stay visible when the shade is
                        // collapsed: back, play/pause, forward.
                        .setShowActionsInCompactView(0, 1, 2),
                )
        } else {
            /*
             * The pass, with a bar and the only way to be rid of it.
             *
             * `setOngoing` so it cannot be swiped away: the work carries on
             * either way, and a notification that can be dismissed while the
             * thing it reports keeps running is a notification that lies. Stop
             * is therefore the only way to dismiss it, which is also the only
             * honest one — it ends the work the notification is about.
             *
             * Transport controls are left off entirely here: there is nothing
             * playing to skip.
             */
            builder
                .setOngoing(true)
                .setProgress(analysedTotal.coerceAtLeast(1), analysedDone, analysedTotal == 0)
                .addAction(
                    android.R.drawable.ic_menu_close_clear_cancel,
                    "Stop",
                    stopAnalysisIntent(),
                )
        }

        return builder.build()
    }

    /** `startForeground`, with the type Android 14 and up require. */
    private fun enterForeground(notification: Notification) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(
                NOTIFICATION_ID,
                notification,
                android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK,
            )
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
    }

    override fun onDestroy() {
        started = false
        if (wakeLock?.isHeld == true) wakeLock?.release()
        wakeLock = null
        session?.isActive = false
        session?.release()
        session = null
        super.onDestroy()
    }

    /**
     * Implemented in `src/android.rs`.
     *
     * The name is the JNI mangling of this class and method, so **renaming
     * either breaks it silently** — the Kotlin side throws
     * `UnsatisfiedLinkError` the first time a button is pressed.
     */
    private external fun onMediaButton(press: Int)

    /**
     * Implemented in `src/android.rs`. Stops the analysis pass.
     *
     * Same naming contract as [onMediaButton]: the JNI symbol mangles to this
     * class and method, so renaming either breaks it silently.
     */
    private external fun onStopAnalysis()

    /** The Stop button on the pass notification, as an intent back to here. */
    private fun stopAnalysisIntent(): PendingIntent = PendingIntent.getService(
        this,
        1,
        Intent(this, PlaybackService::class.java).putExtra(EXTRA_KIND, KIND_STOP_ANALYSIS),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )

    companion object {
        private const val CHANNEL = "vapor.playback"
        private const val NOTIFICATION_ID = 1

        private const val EXTRA_KIND = "kind"
        private const val KIND_ANALYSIS = "analysis"
        private const val KIND_STOP_ANALYSIS = "stop-analysis"
        private const val EXTRA_ACTIVE = "active"
        private const val EXTRA_DONE = "done"
        private const val EXTRA_TOTAL = "total"

        private const val EXTRA_TITLE = "title"
        private const val EXTRA_ARTIST = "artist"
        private const val EXTRA_PLAYING = "playing"
        private const val EXTRA_POSITION = "position"
        private const val EXTRA_DURATION = "duration"

        /** Matches `media::Press` in the Rust side. */
        private const val PRESS_PLAY = 0
        private const val PRESS_PAUSE = 1
        private const val PRESS_NEXT = 3
        private const val PRESS_PREVIOUS = 4

        /**
         * A ceiling on the wake lock, in case a crash takes the release with
         * it. Long enough not to interrupt a set; finite so a bug cannot hold
         * the CPU awake until the battery is flat.
         */
        private const val WAKE_LOCK_LIMIT = 6L * 60L * 60L * 1000L

        init {
            // The same library `MainActivity` loads. Calling
            // `onMediaButton` before it is loaded would throw.
            System.loadLibrary("vapor_app_lib")
        }

        /** Called from Rust. Starts the service if it is not running. */
        @JvmStatic
        fun update(
            context: Context,
            title: String,
            artist: String,
            playing: Boolean,
            position: Long,
            duration: Long,
        ) {
            val intent = Intent(context, PlaybackService::class.java).apply {
                putExtra(EXTRA_TITLE, title)
                putExtra(EXTRA_ARTIST, artist)
                putExtra(EXTRA_PLAYING, playing)
                putExtra(EXTRA_POSITION, position)
                putExtra(EXTRA_DURATION, duration)
            }
            send(context, intent, wanted = playing || title.isNotEmpty())
        }

        /**
         * Called from Rust while the analysis pass is running.
         *
         * The pass is minutes to hours of downloading, and a backgrounded app
         * is frozen — so without this it simply stopped the moment someone
         * switched away, which is what it did.
         */
        @JvmStatic
        fun analysis(context: Context, done: Int, total: Int, active: Boolean) {
            val intent = Intent(context, PlaybackService::class.java).apply {
                putExtra(EXTRA_KIND, KIND_ANALYSIS)
                putExtra(EXTRA_ACTIVE, active)
                putExtra(EXTRA_DONE, done)
                putExtra(EXTRA_TOTAL, total)
            }
            send(context, intent, wanted = active)
        }

        /**
         * Called from Rust when there is nothing playing.
         *
         * Not `stopService`: a pass may still be running, and the service
         * decides for itself when neither reason is left.
         */
        @JvmStatic
        fun stop(context: Context) {
            update(context, "", "", false, 0L, 0L)
        }

        /**
         * Whether the service is up, or on its way up.
         *
         * Set when a delivery is sent rather than in `onCreate`, so a stop that
         * overtakes a start still reaches the service instead of being dropped
         * in the window before it exists.
         */
        @Volatile
        private var started = false

        /**
         * Deliver [intent], starting the service if there is a reason for it to
         * be running.
         *
         * A delivery that says "nothing is happening" to a service that is not
         * running is nothing at all. It used to be a `startForegroundService`
         * followed immediately by `stopSelf`, which is the fatal pair described
         * in [onStartCommand]: on a cold start Rust publishes "nothing playing"
         * before anything has played, so the app killed itself on launch every
         * time. [onStartCommand] keeps the contract on its own now; this keeps
         * the promise from being made when there was never anything to promise.
         */
        private fun send(context: Context, intent: Intent, wanted: Boolean) {
            if (!wanted && !started) return
            started = true
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent)
            } else {
                context.startService(intent)
            }
        }

        private fun PlaybackService.action(action: Long): PendingIntent =
            MediaButtonReceiver.buildMediaButtonPendingIntent(this, action)
    }
}
