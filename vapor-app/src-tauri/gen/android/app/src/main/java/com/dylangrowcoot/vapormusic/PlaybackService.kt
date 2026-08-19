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

        val title = intent?.getStringExtra(EXTRA_TITLE) ?: ""
        val artist = intent?.getStringExtra(EXTRA_ARTIST) ?: ""
        val playing = intent?.getBooleanExtra(EXTRA_PLAYING, false) ?: false
        val position = intent?.getLongExtra(EXTRA_POSITION, 0L) ?: 0L
        val duration = intent?.getLongExtra(EXTRA_DURATION, 0L) ?: 0L

        publish(title, artist, playing, position, duration)

        if (playing) {
            if (wakeLock?.isHeld == false) wakeLock?.acquire(WAKE_LOCK_LIMIT)
        } else if (wakeLock?.isHeld == true) {
            wakeLock?.release()
        }

        // Sticky would have Android restart this with a null intent after a
        // kill, which would put up a notification for a track that is not
        // playing. There is nothing to resume without the app.
        return START_NOT_STICKY
    }

    private fun publish(
        title: String,
        artist: String,
        playing: Boolean,
        position: Long,
        duration: Long,
    ) {
        val session = session ?: return

        session.setMetadata(
            MediaMetadataCompat.Builder()
                .putString(MediaMetadataCompat.METADATA_KEY_TITLE, title)
                .putString(MediaMetadataCompat.METADATA_KEY_ARTIST, artist)
                .putLong(MediaMetadataCompat.METADATA_KEY_DURATION, duration)
                .build(),
        )

        session.setPlaybackState(
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

        val open = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        val notification: Notification = NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(android.R.drawable.ic_media_play)
            .setContentTitle(title.ifEmpty { "Vapor Music" })
            .setContentText(artist)
            .setContentIntent(open)
            .setOnlyAlertOnce(true)
            .setSilent(true)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
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
                    .setMediaSession(session.sessionToken)
                    // Which of the three stay visible when the shade is
                    // collapsed: back, play/pause, forward.
                    .setShowActionsInCompactView(0, 1, 2),
            )
            .build()

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

    companion object {
        private const val CHANNEL = "vapor.playback"
        private const val NOTIFICATION_ID = 1

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
            // `startForegroundService` promises a `startForeground` within a few
            // seconds, which `onStartCommand` does on every delivery.
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent)
            } else {
                context.startService(intent)
            }
        }

        /** Called from Rust when there is nothing playing to keep alive for. */
        @JvmStatic
        fun stop(context: Context) {
            context.stopService(Intent(context, PlaybackService::class.java))
        }

        private fun PlaybackService.action(action: Long): PendingIntent =
            MediaButtonReceiver.buildMediaButtonPendingIntent(this, action)
    }
}
