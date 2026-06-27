#ifndef AUDIO_DSP_H
#define AUDIO_DSP_H

#include <godot_cpp/classes/node.hpp>
#include <godot_cpp/core/class_db.hpp>
#include <godot_cpp/variant/dictionary.hpp>
#include <godot_cpp/variant/string.hpp>
#include <godot_cpp/variant/packed_byte_array.hpp>
#include <godot_cpp/variant/packed_float32_array.hpp>
#include <godot_cpp/variant/packed_vector2_array.hpp>
#include <vector>
#include <thread>
#include <mutex>
#include <condition_variable>
#include <atomic>
#include <chrono>

namespace RubberBand {
class RubberBandStretcher;
}

namespace godot {

class RingBuffer {
public:
	RingBuffer(size_t capacity = 65536) : m_capacity(capacity), m_buffer(capacity), m_head(0), m_tail(0), m_size(0) {}

	void resize(size_t capacity) {
		m_capacity = capacity;
		m_buffer.resize(capacity);
		clear();
	}

	size_t push(const float* data, size_t count) {
		size_t written = 0;
		while (written < count && m_size < m_capacity) {
			m_buffer[m_head] = data[written];
			m_head = (m_head + 1) % m_capacity;
			m_size++;
			written++;
		}
		return written;
	}

	size_t pop(float* data, size_t count) {
		size_t read = 0;
		while (read < count && m_size > 0) {
			data[read] = m_buffer[m_tail];
			m_tail = (m_tail + 1) % m_capacity;
			m_size--;
			read++;
		}
		return read;
	}

	size_t size() const { return m_size; }
	size_t capacity() const { return m_capacity; }
	size_t available_write() const { return m_capacity - m_size; }
	void clear() {
		m_head = 0;
		m_tail = 0;
		m_size = 0;
	}

private:
	size_t m_capacity;
	std::vector<float> m_buffer;
	size_t m_head;
	size_t m_tail;
	size_t m_size;
};

class AudioDSP : public Node {
	GDCLASS(AudioDSP, Node);

protected:
	static void _bind_methods();

public:
	AudioDSP();
	~AudioDSP();

	String get_library_version() const;
	Dictionary analyze_file(String file_path);
	PackedFloat32Array stretch_buffer(PackedFloat32Array input_samples, float ratio);
	Dictionary analyze_samples(PackedFloat32Array samples, float sample_rate);

	// Streaming & Real-time Time Stretching
	bool load_file(String file_path, float duration_hint = 0.0f);
	PackedVector2Array get_next_chunk(int num_frames, float ratio);
	void seek_pos(float position_seconds);
	float get_playback_position() const;
	float get_duration() const;
	bool is_finished() const;
	void clear_stream();
	int get_cache_sample_count() const;
	float get_cross_correlation_offset(AudioDSP* other_dsp, float self_time, float other_time, float window_size_sec);
	std::vector<float> get_samples_in_range(float start_time, float duration_sec);
	PackedFloat32Array get_waveform_peaks(int num_bins);

private:
	Dictionary _analyze_samples_impl(const std::vector<float>& samples, float sample_rate);
	void load_cache_at(size_t start_idx);
	float get_wav_duration(const std::string& filepath);
	float get_metadata_duration(const std::string& filepath);
	int get_wav_channels(const std::string& filepath);
	int get_audio_channels(const std::string& filepath);
	int get_channels_via_ffprobe(const std::string& filepath);
	bool downmix_wav_to_mono(const std::string& input_path, const std::string& output_path);
	bool downmix_via_ffmpeg(const std::string& input_path, const std::string& output_path);

	std::string m_temp_mono_path;

	std::string m_file_path;
	size_t m_total_samples = 0;
	size_t m_cache_start_idx = 0;

	std::vector<float> m_samples;
	size_t m_read_idx = 0;
	RubberBand::RubberBandStretcher* m_stretcher = nullptr;
	float m_duration = 0.0f;

	// Threaded streaming support
	void start_thread();
	void stop_thread();
	void thread_loop();

	std::thread m_thread;
	mutable std::mutex m_mutex;
	std::condition_variable m_cond;
	bool m_thread_running = false;
	
	// Seeking in background thread
	bool m_seek_pending = false;
	size_t m_seek_target_idx = 0;

	// Playback tracking & ratio
	float m_playback_sec = 0.0f;
	std::atomic<float> m_ratio{1.0f};

	// Ring buffer
	RingBuffer m_ring_buffer;
};

}

#endif // AUDIO_DSP_H
