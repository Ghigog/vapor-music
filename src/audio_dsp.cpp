#include "audio_dsp.h"

#include <godot_cpp/core/class_db.hpp>
#include <godot_cpp/variant/utility_functions.hpp>
#include <essentia/algorithmfactory.h>
#include <essentia/pool.h>
#include <rubberband/RubberBandStretcher.h>
#include <cmath>
#include <complex>
#include <vector>
#include <string>
#include <algorithm>
#include <fstream>
#include <cstdio>
#include <cstdint>
#include <unistd.h>

using namespace godot;

void AudioDSP::_bind_methods() {
	ClassDB::bind_method(D_METHOD("get_library_version"), &AudioDSP::get_library_version);
	ClassDB::bind_method(D_METHOD("analyze_file", "file_path"), &AudioDSP::analyze_file);
	ClassDB::bind_method(D_METHOD("stretch_buffer", "input_samples", "ratio"), &AudioDSP::stretch_buffer);
	ClassDB::bind_method(D_METHOD("analyze_samples", "samples", "sample_rate"), &AudioDSP::analyze_samples);

	ClassDB::bind_method(D_METHOD("load_file", "file_path", "duration_hint"), &AudioDSP::load_file, DEFVAL(0.0f));
	ClassDB::bind_method(D_METHOD("get_next_chunk", "num_frames", "ratio"), &AudioDSP::get_next_chunk);
	ClassDB::bind_method(D_METHOD("seek_pos", "position_seconds"), &AudioDSP::seek_pos);
	ClassDB::bind_method(D_METHOD("get_playback_position"), &AudioDSP::get_playback_position);
	ClassDB::bind_method(D_METHOD("get_duration"), &AudioDSP::get_duration);
	ClassDB::bind_method(D_METHOD("is_finished"), &AudioDSP::is_finished);
	ClassDB::bind_method(D_METHOD("clear_stream"), &AudioDSP::clear_stream);
	ClassDB::bind_method(D_METHOD("get_cache_sample_count"), &AudioDSP::get_cache_sample_count);
	ClassDB::bind_method(D_METHOD("get_cross_correlation_offset", "other_dsp", "self_time", "other_time", "window_size_sec"), &AudioDSP::get_cross_correlation_offset);
	ClassDB::bind_method(D_METHOD("get_waveform_peaks", "num_bins"), &AudioDSP::get_waveform_peaks);
	ClassDB::bind_method(D_METHOD("calculate_chunk_rms", "chunk"), &AudioDSP::calculate_chunk_rms);
}

AudioDSP::AudioDSP() {
	m_stretcher = nullptr;
	m_read_idx = 0;
	m_duration = 0.0f;
	m_total_samples = 0;
	m_cache_start_idx = 0;
	m_playback_sec = 0.0f;
	m_thread_running = false;
	m_seek_pending = false;
	m_seek_target_idx = 0;
	m_ratio.store(1.0f);
	m_temp_mono_path = "";
}
AudioDSP::~AudioDSP() {
	clear_stream();
}

String AudioDSP::get_library_version() const {
	return "Essentia v2.1 / Rubber Band v3.3";
}

namespace {
const double PI = 3.14159265358979323846;
typedef std::complex<double> Complex;

// In-place Cooley-Tukey FFT
void fft(std::vector<Complex>& a) {
	int n = a.size();
	for (int i = 1, j = 0; i < n; i++) {
		int bit = n >> 1;
		for (; j & bit; bit >>= 1) j ^= bit;
		j ^= bit;
		if (i < j) std::swap(a[i], a[j]);
	}
	for (int len = 2; len <= n; len <<= 1) {
		double angle = -2 * PI / len;
		Complex wlen(std::cos(angle), std::sin(angle));
		for (int i = 0; i < n; i += len) {
			Complex w(1);
			for (int j = 0; j < len / 2; j++) {
				Complex u = a[i + j];
				Complex v = a[i + j + len / 2] * w;
				a[i + j] = u + v;
				a[i + j + len / 2] = u - v;
				w *= wlen;
			}
		}
	}
}
}

static std::string extract_key_from_segment(const std::vector<float>& audio, float start_sec, float end_sec, const Dictionary& camelot_map) {
	int start_sample = std::clamp(static_cast<int>(start_sec * 44100.0f), 0, static_cast<int>(audio.size()));
	int end_sample = std::clamp(static_cast<int>(end_sec * 44100.0f), 0, static_cast<int>(audio.size()));
	
	if (end_sample - start_sample < 44100 * 3) {
		return "";
	}
	
	std::vector<float> segment_audio(audio.begin() + start_sample, audio.begin() + end_sample);
	
	try {
		essentia::standard::Algorithm* key_extractor = essentia::standard::AlgorithmFactory::create("KeyExtractor");
		key_extractor->configure("profileType", "temperley");
		
		std::string key_name;
		std::string scale_name;
		float key_strength = 0.0f;
		
		key_extractor->input("audio").set(segment_audio);
		key_extractor->output("key").set(key_name);
		key_extractor->output("scale").set(scale_name);
		key_extractor->output("strength").set(key_strength);
		key_extractor->compute();
		
		delete key_extractor;
		
		std::string key_query = key_name + " " + scale_name;
		if (camelot_map.has(key_query.c_str())) {
			String val = camelot_map[key_query.c_str()];
			return val.utf8().get_data();
		}
	} catch (...) {
		// Ignore and fallback
	}
	return "";
}

Dictionary AudioDSP::analyze_file(String file_path) {
	Dictionary results;
	
	std::string path_std = file_path.utf8().get_data();
	std::string target_path = path_std;
	std::string temp_mono_path = "";
	bool is_temp = false;

	int channels = get_audio_channels(path_std);
	if (channels > 2) {
		std::string filename = path_std;
		size_t last_slash = filename.find_last_of("/\\");
		if (last_slash != std::string::npos) {
			filename = filename.substr(last_slash + 1);
		}
		temp_mono_path = "/tmp/" + filename + ".mono.wav";
		bool downmix_ok = false;
		
		std::string path_lower = path_std;
		std::transform(path_lower.begin(), path_lower.end(), path_lower.begin(), ::tolower);
		if (path_lower.size() >= 4 && path_lower.substr(path_lower.size() - 4) == ".wav") {
			downmix_ok = downmix_wav_to_mono(path_std, temp_mono_path);
		}
		if (!downmix_ok) {
			downmix_ok = downmix_via_ffmpeg(path_std, temp_mono_path);
		}
		
		if (downmix_ok) {
			target_path = temp_mono_path;
			is_temp = true;
		}
	}
	
	try {
		// 1. Load audio using MonoLoader
		essentia::standard::Algorithm* loader = essentia::standard::AlgorithmFactory::create("MonoLoader");
		loader->configure("filename", target_path);
		loader->configure("sampleRate", 44100.0f);
		
		std::vector<float> audio;
		loader->output("audio").set(audio);
		loader->compute();
		
		delete loader;
		
		if (audio.empty()) {
			UtilityFunctions::printerr("AudioDSP: Failed to load audio or file is empty: ", file_path);
			if (is_temp) {
				std::remove(temp_mono_path.c_str());
			}
			return results;
		}
		
		float duration = static_cast<float>(audio.size()) / 44100.0f;
		
		// 2. Estimate Key and Scale using KeyExtractor
		essentia::standard::Algorithm* key_extractor = essentia::standard::AlgorithmFactory::create("KeyExtractor");
		key_extractor->configure("profileType", "temperley");
		
		std::string key_name;
		std::string scale_name;
		float key_strength = 0.0f;
		
		key_extractor->input("audio").set(audio);
		key_extractor->output("key").set(key_name);
		key_extractor->output("scale").set(scale_name);
		key_extractor->output("strength").set(key_strength);
		key_extractor->compute();
		
		delete key_extractor;
		
		// Map scale/key to Camelot notation
		Dictionary camelot_map;
		camelot_map["Ab major"] = "4B"; camelot_map["Ab minor"] = "1A";
		camelot_map["A major"] = "11B"; camelot_map["A minor"] = "8A";
		camelot_map["Bb major"] = "6B"; camelot_map["Bb minor"] = "3A";
		camelot_map["B major"] = "1B"; camelot_map["B minor"] = "10A";
		camelot_map["C major"] = "8B"; camelot_map["C minor"] = "5A";
		camelot_map["C# major"] = "3B"; camelot_map["C# minor"] = "12A";
		camelot_map["Db major"] = "3B"; camelot_map["Db minor"] = "12A";
		camelot_map["D major"] = "10B"; camelot_map["D minor"] = "7A";
		camelot_map["Eb major"] = "5B"; camelot_map["Eb minor"] = "2A";
		camelot_map["E major"] = "12B"; camelot_map["E minor"] = "9A";
		camelot_map["F major"] = "7B"; camelot_map["F minor"] = "4A";
		camelot_map["F# major"] = "2B"; camelot_map["F# minor"] = "11A";
		camelot_map["Gb major"] = "2B"; camelot_map["Gb minor"] = "11A";
		camelot_map["G major"] = "9B"; camelot_map["G minor"] = "6A";
		
		std::string key_query = key_name + " " + scale_name;
		String camelot_key = "8A"; // Fallback to A minor
		if (camelot_map.has(key_query.c_str())) {
			camelot_key = camelot_map[key_query.c_str()];
		}
		
		// 3. Estimate BPM and Beat Grid using RhythmExtractor2013
		float bpm = 120.0f;
		std::vector<float> ticks;
		float rhythm_confidence = 0.0f;
		std::vector<float> estimates;
		std::vector<float> bpm_intervals;
		
		essentia::standard::Algorithm* rhythm = essentia::standard::AlgorithmFactory::create("RhythmExtractor2013");
		
		rhythm->input("signal").set(audio);
		rhythm->output("bpm").set(bpm);
		rhythm->output("ticks").set(ticks);
		rhythm->output("confidence").set(rhythm_confidence);
		rhythm->output("estimates").set(estimates);
		rhythm->output("bpmIntervals").set(bpm_intervals);
		rhythm->compute();
		
		delete rhythm;
		
		// Format beat grid and downbeats
		PackedFloat32Array beat_grid;
		PackedFloat32Array downbeats;
		
		for (size_t i = 0; i < ticks.size(); ++i) {
			beat_grid.append(ticks[i]);
			if (i % 4 == 0) {
				downbeats.append(ticks[i]);
			}
		}
		
		// If beat grid is empty, generate a fallback grid based on estimated BPM
		if (beat_grid.size() == 0) {
			float beat_interval = 60.0f / bpm;
			float t = 0.0f;
			int idx = 0;
			while (t < duration) {
				beat_grid.append(t);
				if (idx % 4 == 0) {
					downbeats.append(t);
				}
				t += beat_interval;
				idx++;
			}
		}
		
		// 4. Calculate Perceived Energy Level & Structural Boundaries
		// Compute RMS envelope in 1-second windows
		int window_size = 44100; // 1 second
		int num_windows = audio.size() / window_size;
		std::vector<float> rms_envelope;
		float max_rms = 0.0f;
		float total_rms = 0.0f;
		
		for (int i = 0; i < num_windows; ++i) {
			float sum_squares = 0.0f;
			for (int j = 0; j < window_size; ++j) {
				float val = audio[i * window_size + j];
				sum_squares += val * val;
			}
			float rms = std::sqrt(sum_squares / window_size);
			rms_envelope.push_back(rms);
			if (rms > max_rms) {
				max_rms = rms;
			}
			total_rms += rms;
		}
		
		// Perceived energy level
		float energy_level = 0.5f;
		if (max_rms > 0.001f) {
			energy_level = std::clamp((total_rms / num_windows) / max_rms, 0.0f, 1.0f);
		}
		
		// Generate an energy graph (20 points for UI)
		PackedFloat32Array energy_graph;
		int points = 20;
		int step = std::max(1, num_windows / points);
		for (int i = 0; i < points; ++i) {
			int idx = std::min(i * step, static_cast<int>(rms_envelope.size() - 1));
			if (idx >= 0 && max_rms > 0.001f) {
				energy_graph.append(rms_envelope[idx] / max_rms);
			} else {
				energy_graph.append(0.5f);
			}
		}
		
		// Determine boundaries for intro, outro using Spectral Centroid and transient density
		float intro_end = 30.0f; // Default 30 seconds
		float outro_start = duration - 30.0f; // Default last 30 seconds

		if (num_windows >= 3 && audio.size() >= 1024) {
			// Precompute Hanning window
			std::vector<double> hanning(1024);
			for (int n = 0; n < 1024; ++n) {
				hanning[n] = 0.5 * (1.0 - std::cos(2.0 * PI * n / 1023.0));
			}

			// Frame-by-frame analysis
			int fft_size = 1024;
			int hop_size = 512;
			int num_frames = (audio.size() - fft_size) / hop_size + 1;

			std::vector<float> frame_sc(num_frames, 0.0f);
			std::vector<float> frame_hfc(num_frames, 0.0f);
			std::vector<Complex> fft_buffer(fft_size);

			for (int f = 0; f < num_frames; ++f) {
				int start_idx = f * hop_size;
				for (int n = 0; n < fft_size; ++n) {
					fft_buffer[n] = audio[start_idx + n] * hanning[n];
				}

				fft(fft_buffer);

				double sc_numerator = 0.0;
				double sc_denominator = 0.0;
				double hfc_sum = 0.0;

				for (int k = 0; k <= fft_size / 2; ++k) {
					double mag = std::abs(fft_buffer[k]);
					double freq = k * 44100.0 / fft_size;

					sc_numerator += freq * mag;
					sc_denominator += mag;
					hfc_sum += k * mag;
				}

				frame_sc[f] = (sc_denominator > 1e-6) ? static_cast<float>(sc_numerator / sc_denominator) : 0.0f;
				frame_hfc[f] = static_cast<float>(hfc_sum);
			}

			// Onset detection function (first-order difference of HFC)
			std::vector<float> onset_strength(num_frames, 0.0f);
			float max_os = 0.0f;
			for (int f = 1; f < num_frames; ++f) {
				onset_strength[f] = std::max(0.0f, frame_hfc[f] - frame_hfc[f-1]);
				if (onset_strength[f] > max_os) {
					max_os = onset_strength[f];
				}
			}

			// Detect transient frames
			std::vector<bool> is_transient(num_frames, false);
			float os_threshold = max_os * 0.05f;
			if (os_threshold < 10.0f) os_threshold = 10.0f;

			for (int f = 1; f < num_frames - 1; ++f) {
				if (onset_strength[f] > os_threshold &&
					onset_strength[f] > onset_strength[f-1] &&
					onset_strength[f] > onset_strength[f+1]) {
					is_transient[f] = true;
				}
			}

			// Aggregate to 1-second windows
			std::vector<float> window_sc(num_windows, 0.0f);
			std::vector<int> window_sc_count(num_windows, 0);
			std::vector<int> window_transients(num_windows, 0);

			for (int f = 0; f < num_frames; ++f) {
				int win_idx = static_cast<int>((f * hop_size) / 44100.0);
				if (win_idx >= 0 && win_idx < num_windows) {
					window_sc[win_idx] += frame_sc[f];
					window_sc_count[win_idx]++;
					if (is_transient[f]) {
						window_transients[win_idx]++;
					}
				}
			}

			for (int i = 0; i < num_windows; ++i) {
				if (window_sc_count[i] > 0) {
					window_sc[i] /= window_sc_count[i];
				}
			}

			// Intro end detection: first window where RMS exceeds 15%, Spectral Centroid > 800 Hz, and transient density (sum of 3s) >= 3
			for (int i = 0; i < num_windows - 2; ++i) {
				int transient_sum = window_transients[i] + window_transients[i+1] + window_transients[i+2];
				if (rms_envelope[i] > max_rms * 0.15f &&
					window_sc[i] > 800.0f &&
					transient_sum >= 3) {
					intro_end = static_cast<float>(i);
					break;
				}
			}

			// Outro start detection: last window from end where rhythmic activity is still present
			for (int i = num_windows - 3; i >= 0; --i) {
				int transient_sum = window_transients[i] + window_transients[i+1] + window_transients[i+2];
				if (transient_sum >= 3 &&
					rms_envelope[i] > max_rms * 0.15f &&
					window_sc[i] > 800.0f) {
					outro_start = static_cast<float>(i + 3);
					break;
				}
			}
		}

		// Clamp boundaries safely
		float intro_high_bound = std::min(60.0f, duration * 0.25f);
		float intro_low_bound = 0.0f;
		if (intro_high_bound < intro_low_bound) {
			intro_high_bound = intro_low_bound;
		}
		intro_end = std::clamp(intro_end, intro_low_bound, intro_high_bound);

		float outro_low_bound = duration * 0.70f;
		float outro_high_bound = std::max(outro_low_bound, duration - 5.0f);
		outro_start = std::clamp(outro_start, outro_low_bound, outro_high_bound);
		
		Dictionary segments;
		PackedFloat32Array intro_seg;
		intro_seg.append(0.0f);
		intro_seg.append(intro_end);
		segments["intro"] = intro_seg;
		
		PackedFloat32Array outro_seg;
		outro_seg.append(outro_start);
		outro_seg.append(duration);
		segments["outro"] = outro_seg;
		
		PackedFloat32Array drop_seg;
		drop_seg.append(intro_end);
		drop_seg.append(outro_start);
		segments["drop"] = drop_seg;
		
		// 5. Calculate Silence Trimming & Loudness Normalization
		Dictionary norm_props = _analyze_samples_impl(audio, 44100.0f);
		
		// 5b. Analyze per-segment key signatures
		std::string intro_key_str = extract_key_from_segment(audio, 0.0f, intro_end, camelot_map);
		std::string outro_key_str = extract_key_from_segment(audio, outro_start, duration, camelot_map);
		String ik = intro_key_str.empty() ? camelot_key : String(intro_key_str.c_str());
		String ok = outro_key_str.empty() ? camelot_key : String(outro_key_str.c_str());
		
		Dictionary segment_keys;
		segment_keys["intro"] = ik;
		segment_keys["outro"] = ok;
		
		// 5c. Perform transient peak detection
		int hop_size = 2205; // 50 ms
		int num_hops = audio.size() / hop_size;
		std::vector<float> hop_energies;
		hop_energies.reserve(num_hops);
		float max_energy = 0.0f;
		
		for (int i = 0; i < num_hops; ++i) {
			float sum_sq = 0.0f;
			for (int j = 0; j < hop_size; ++j) {
				float val = audio[i * hop_size + j];
				sum_sq += val * val;
			}
			float rms = std::sqrt(sum_sq / hop_size);
			hop_energies.push_back(rms);
			if (rms > max_energy) {
				max_energy = rms;
			}
		}
		
		PackedFloat32Array transients;
		float energy_threshold = max_energy * 0.7f;
		for (size_t i = 1; i < hop_energies.size() - 1; ++i) {
			if (hop_energies[i] > energy_threshold && 
				hop_energies[i] > hop_energies[i-1] && 
				hop_energies[i] > hop_energies[i+1]) {
				float t_time = static_cast<float>(i * hop_size) / 44100.0f;
				transients.append(t_time);
			}
		}
		
		// 6. Populate Results
		results["bpm"] = bpm;
		results["musical_key"] = camelot_key;
		results["intro_key"] = ik;
		results["outro_key"] = ok;
		results["segment_keys"] = segment_keys;
		results["transients"] = transients;
		results["energy_level"] = energy_level;
		results["energy_graph"] = energy_graph;
		results["beat_grid"] = beat_grid;
		results["downbeats"] = downbeats;
		results["segments"] = segments;
		results["duration"] = duration;
		results["cue_in"] = norm_props["cue_in"];
		results["cue_out"] = norm_props["cue_out"];
		results["lufs"] = norm_props["lufs"];
		
	} catch (const std::exception& e) {
		UtilityFunctions::printerr("AudioDSP: Exception during Essentia analysis: ", e.what());
	}
	
	if (is_temp) {
		std::remove(temp_mono_path.c_str());
	}
	
	return results;
}

PackedFloat32Array AudioDSP::stretch_buffer(PackedFloat32Array input_samples, float ratio) {
	PackedFloat32Array output_samples;
	if (input_samples.size() == 0 || ratio <= 0.0f) {
		return output_samples;
	}

	size_t num_samples = input_samples.size();
	RubberBand::RubberBandStretcher stretcher(
		44100, 
		1, 
		RubberBand::RubberBandStretcher::DefaultOptions | RubberBand::RubberBandStretcher::OptionProcessOffline
	);
	
	stretcher.setTimeRatio(ratio);
	
	const float* input_ptr = input_samples.ptr();
	const float* input_buffers[1] = { input_ptr };
	
	stretcher.process(input_buffers, num_samples, true);
	
	int available = stretcher.available();
	if (available > 0) {
		output_samples.resize(available);
		float* output_ptr = output_samples.ptrw();
		float* output_buffers[1] = { output_ptr };
		stretcher.retrieve(output_buffers, available);
	}
	
	return output_samples;
}

Dictionary AudioDSP::analyze_samples(PackedFloat32Array samples, float sample_rate) {
	std::vector<float> sample_vector;
	sample_vector.resize(samples.size());
	for (int64_t i = 0; i < samples.size(); ++i) {
		sample_vector[i] = samples[i];
	}
	return _analyze_samples_impl(sample_vector, sample_rate);
}

Dictionary AudioDSP::_analyze_samples_impl(const std::vector<float>& samples, float sample_rate) {
	Dictionary results;
	
	// 1. Silence Detection
	int window_size = 441; // 10 ms at 44.1 kHz
	if (sample_rate != 44100.0f) {
		window_size = std::max(1, static_cast<int>(std::round(0.01 * sample_rate)));
	}
	int num_windows = samples.size() / window_size;
	double threshold = 0.01; // -40 dB
	
	double cue_in = 0.0;
	double cue_out = static_cast<double>(samples.size()) / sample_rate;
	
	bool found_cue_in = false;
	for (int i = 0; i < num_windows; ++i) {
		double sum_sq = 0.0;
		for (int j = 0; j < window_size; ++j) {
			float val = samples[i * window_size + j];
			sum_sq += val * val;
		}
		double rms = std::sqrt(sum_sq / window_size);
		if (rms > threshold) {
			cue_in = static_cast<double>(i * window_size) / sample_rate;
			found_cue_in = true;
			break;
		}
	}
	
	for (int i = num_windows - 1; i >= 0; --i) {
		double sum_sq = 0.0;
		for (int j = 0; j < window_size; ++j) {
			float val = samples[i * window_size + j];
			sum_sq += val * val;
		}
		double rms = std::sqrt(sum_sq / window_size);
		if (rms > threshold) {
			cue_out = static_cast<double>((i + 1) * window_size) / sample_rate;
			break;
		}
	}
	
	// 2. EBU R128 Loudness
	double lufs = -70.0;
	if (!samples.empty()) {
		double f0 = 1681.974450955533;
		double G = 3.999843853973347;
		double Q = 0.7071752369554196;

		double K = tan(M_PI * f0 / (double)sample_rate);
		double Vh = pow(10.0, G / 20.0);
		double Vb = pow(Vh, 0.4996667741545416);

		double a0 = 1.0 + K / Q + K * K;
		double pb[3];
		double pa[3];
		pb[0] = (Vh + Vb * K / Q + K * K) / a0;
		pb[1] = 2.0 * (K * K - Vh) / a0;
		pb[2] = (Vh - Vb * K / Q + K * K) / a0;
		pa[0] = 1.0;
		pa[1] = 2.0 * (K * K - 1.0) / a0;
		pa[2] = (1.0 - K / Q + K * K) / a0;

		f0 = 38.13547087602444;
		Q = 0.5003270373238773;
		K = tan(M_PI * f0 / (double)sample_rate);

		double a0_hp = 1.0 + K / Q + K * K;
		double rb[3] = { 1.0, -2.0, 1.0 };
		double ra[3];
		ra[0] = 1.0;
		ra[1] = 2.0 * (K * K - 1.0) / a0_hp;
		ra[2] = (1.0 - K / Q + K * K) / a0_hp;
		
		std::vector<double> filtered(samples.size());
		double x1 = 0.0, x2 = 0.0, y1 = 0.0, y2 = 0.0;
		double r1 = 0.0, r2 = 0.0, s1 = 0.0, s2 = 0.0;

		for (size_t n = 0; n < samples.size(); ++n) {
			double x = samples[n];
			
			// Stage 1 (shelving)
			double y_stage1 = pb[0] * x + pb[1] * x1 + pb[2] * x2 - pa[1] * y1 - pa[2] * y2;
			x2 = x1;
			x1 = x;
			y2 = y1;
			y1 = y_stage1;

			// Stage 2 (high pass)
			double y_stage2 = rb[0] * y_stage1 + rb[1] * r1 + rb[2] * r2 - ra[1] * s1 - ra[2] * s2;
			r2 = r1;
			r1 = y_stage1;
			s2 = s1;
			s1 = y_stage2;

			filtered[n] = y_stage2;
		}

		int n400 = std::round(0.4 * sample_rate);
		int n100 = std::round(0.1 * sample_rate);
		std::vector<double> block_energies;
		
		if (filtered.size() < static_cast<size_t>(n400)) {
			double sum_sq = 0.0;
			for (double val : filtered) {
				sum_sq += val * val;
			}
			block_energies.push_back(sum_sq / filtered.size());
		} else {
			for (size_t start = 0; start + n400 <= filtered.size(); start += n100) {
				double sum_sq = 0.0;
				for (int i = 0; i < n400; ++i) {
					double val = filtered[start + i];
					sum_sq += val * val;
				}
				block_energies.push_back(sum_sq / n400);
			}
		}

		double abs_threshold_energy = std::pow(10.0, (-70.0 + 0.691) / 10.0);
		std::vector<double> J1_energies;
		double sum_J1 = 0.0;
		for (double energy : block_energies) {
			if (energy >= abs_threshold_energy) {
				J1_energies.push_back(energy);
				sum_J1 += energy;
			}
		}

		if (!J1_energies.empty()) {
			double z_avg = sum_J1 / J1_energies.size();
			double rel_threshold_energy = 0.1 * z_avg;
			
			std::vector<double> J2_energies;
			double sum_J2 = 0.0;
			for (double energy : J1_energies) {
				if (energy >= rel_threshold_energy) {
					J2_energies.push_back(energy);
					sum_J2 += energy;
				}
			}
			
			double z_integrated = 0.0;
			if (!J2_energies.empty()) {
				z_integrated = sum_J2 / J2_energies.size();
			} else {
				z_integrated = z_avg;
			}
			
			if (z_integrated > 1e-12) {
				lufs = -0.691 + 10.0 * std::log10(z_integrated);
			}
		}
	}
	
	results["cue_in"] = cue_in;
	results["cue_out"] = cue_out;
	results["lufs"] = lufs;
	
	return results;
}

float AudioDSP::get_wav_duration(const std::string& filepath) {
	std::ifstream file(filepath, std::ios::binary);
	if (!file) return 0.0f;

	char chunk_id[4];
	uint32_t chunk_size = 0;
	char format[4];

	if (!file.read(chunk_id, 4) || !file.read(reinterpret_cast<char*>(&chunk_size), 4) || !file.read(format, 4)) {
		return 0.0f;
	}

	if (std::string(chunk_id, 4) != "RIFF" || std::string(format, 4) != "WAVE") {
		return 0.0f;
	}

	uint32_t sample_rate = 0;
	uint32_t byte_rate = 0;
	uint32_t data_size = 0;

	while (file.read(chunk_id, 4) && file.read(reinterpret_cast<char*>(&chunk_size), 4)) {
		std::string id(chunk_id, 4);

		if (id == "fmt ") {
			uint16_t audio_format = 0;
			uint16_t num_channels = 0;
			uint16_t block_align = 0;
			uint16_t bits_per_sample = 0;

			if (file.read(reinterpret_cast<char*>(&audio_format), 2) &&
				file.read(reinterpret_cast<char*>(&num_channels), 2) &&
				file.read(reinterpret_cast<char*>(&sample_rate), 4) &&
				file.read(reinterpret_cast<char*>(&byte_rate), 4) &&
				file.read(reinterpret_cast<char*>(&block_align), 2) &&
				file.read(reinterpret_cast<char*>(&bits_per_sample), 2)) {
				// Success reading format
			}

			// Skip any extra bytes in fmt chunk if any
			if (chunk_size > 16) {
				file.seekg(chunk_size - 16, std::ios::cur);
			}
		} else if (id == "data") {
			data_size = chunk_size;
			break; // We found the data chunk, can stop
		} else {
			// Skip this chunk
			file.seekg(chunk_size, std::ios::cur);
		}
	}

	if (byte_rate > 0 && data_size > 0) {
		return static_cast<float>(data_size) / byte_rate;
	}
	return 0.0f;
}

float AudioDSP::get_metadata_duration(const std::string& filepath) {
	try {
		essentia::standard::Algorithm* reader = essentia::standard::AlgorithmFactory::create("MetadataReader");
		reader->configure("filename", filepath);
		reader->configure("failOnError", false);

		std::string title, artist, album, comment, genre, tracknumber, date;
		double duration = 0.0;
		int bitrate = 0, sampleRate = 0, channels = 0;
		essentia::Pool tagPool;

		reader->output("title").set(title);
		reader->output("artist").set(artist);
		reader->output("album").set(album);
		reader->output("comment").set(comment);
		reader->output("genre").set(genre);
		reader->output("tracknumber").set(tracknumber);
		reader->output("date").set(date);
		reader->output("duration").set(duration);
		reader->output("bitrate").set(bitrate);
		reader->output("sampleRate").set(sampleRate);
		reader->output("channels").set(channels);
		reader->output("tagPool").set(tagPool);

		reader->compute();
		delete reader;
		return static_cast<float>(duration);
	} catch (...) {
		return 0.0f;
	}
}

void AudioDSP::load_cache_at(size_t start_idx) {
	if (m_file_path.empty() || start_idx >= m_total_samples) {
		std::unique_lock<std::mutex> lock(m_mutex);
		m_samples.clear();
		m_cache_start_idx = start_idx;
		return;
	}

	if (m_duration < 900.0f) {
		{
			std::unique_lock<std::mutex> lock(m_mutex);
			if (m_samples.size() == m_total_samples && m_cache_start_idx == 0 && !m_samples.empty()) {
				return;
			}
		}
		try {
			essentia::standard::Algorithm* loader = essentia::standard::AlgorithmFactory::create("MonoLoader");
			loader->configure("filename", m_file_path);
			loader->configure("sampleRate", 44100.0f);

			std::vector<float> temp_samples;
			loader->output("audio").set(temp_samples);
			loader->compute();
			delete loader;

			std::unique_lock<std::mutex> lock(m_mutex);
			m_samples = std::move(temp_samples);
			m_cache_start_idx = 0;
			m_total_samples = m_samples.size();
			return;
		} catch (...) {
			// Fallback to incremental EasyLoader if MonoLoader fails
		}
	}

	float start_time = static_cast<float>(start_idx) / 44100.0f;
	float end_time = start_time + 5.0f; // 5 seconds cache
	if (end_time > m_duration) {
		end_time = m_duration;
	}

	try {
		essentia::standard::Algorithm* loader = essentia::standard::AlgorithmFactory::create("EasyLoader");
		loader->configure("filename", m_file_path);
		loader->configure("sampleRate", 44100.0f);
		loader->configure("startTime", start_time);
		loader->configure("endTime", end_time);

		std::vector<float> temp_samples;
		loader->output("audio").set(temp_samples);
		loader->compute();

		delete loader;

		std::unique_lock<std::mutex> lock(m_mutex);
		m_samples = std::move(temp_samples);
		m_cache_start_idx = start_idx;
	} catch (...) {
		std::unique_lock<std::mutex> lock(m_mutex);
		m_samples.clear();
		m_cache_start_idx = start_idx;
	}
}

int AudioDSP::get_cache_sample_count() const {
	std::unique_lock<std::mutex> lock(m_mutex);
	return static_cast<int>(m_samples.size());
}

bool AudioDSP::load_file(String file_path, float duration_hint) {
	clear_stream();

	std::string path_std = file_path.utf8().get_data();
	std::string target_path = path_std;

	int channels = get_audio_channels(path_std);
	if (channels > 2) {
		std::string filename = path_std;
		size_t last_slash = filename.find_last_of("/\\");
		if (last_slash != std::string::npos) {
			filename = filename.substr(last_slash + 1);
		}
		m_temp_mono_path = "/tmp/" + filename + ".mono.wav";
		bool downmix_ok = false;
		
		std::string path_lower = path_std;
		std::transform(path_lower.begin(), path_lower.end(), path_lower.begin(), ::tolower);
		if (path_lower.size() >= 4 && path_lower.substr(path_lower.size() - 4) == ".wav") {
			downmix_ok = downmix_wav_to_mono(path_std, m_temp_mono_path);
		}
		if (!downmix_ok) {
			downmix_ok = downmix_via_ffmpeg(path_std, m_temp_mono_path);
		}
		
		if (downmix_ok) {
			target_path = m_temp_mono_path;
		} else {
			m_temp_mono_path = "";
		}
	}

	try {
		// 1. Get the duration using fast duration extractors
		float duration = duration_hint;
		if (duration <= 0.0f) {
			std::string path_lower = target_path;
			std::transform(path_lower.begin(), path_lower.end(), path_lower.begin(), ::tolower);
			if (path_lower.size() >= 4 && path_lower.substr(path_lower.size() - 4) == ".wav") {
				duration = get_wav_duration(target_path);
			} else {
				duration = get_metadata_duration(target_path);
			}

			// Fallback to full MonoLoader if metadata/wav parsing fails
			if (duration <= 0.0f) {
				essentia::standard::Algorithm* loader = essentia::standard::AlgorithmFactory::create("MonoLoader");
				loader->configure("filename", target_path);
				loader->configure("sampleRate", 44100.0f);
				std::vector<float> temp_samples;
				loader->output("audio").set(temp_samples);
				loader->compute();
				duration = static_cast<float>(temp_samples.size()) / 44100.0f;
				delete loader;
			}
		}

		if (duration <= 0.0f) {
			UtilityFunctions::printerr("AudioDSP: Failed to get duration or file is invalid: ", file_path);
			clear_stream();
			return false;
		}

		m_file_path = target_path;
		m_duration = duration;
		m_total_samples = static_cast<size_t>(m_duration * 44100.0f);

		m_read_idx = 0;
		m_playback_sec = 0.0f;

		// Initialize cache - will be loaded asynchronously by the background thread on startup
		m_cache_start_idx = 0;
		m_samples.clear();

		m_stretcher = new RubberBand::RubberBandStretcher(
			44100,
			1,
			RubberBand::RubberBandStretcher::OptionProcessRealTime
		);

		m_ratio.store(1.0f);
		m_ring_buffer.clear();

		start_thread();

		return true;
	} catch (const std::exception& e) {
		UtilityFunctions::printerr("AudioDSP: Exception loading file: ", e.what());
		clear_stream();
		return false;
	}
}

PackedVector2Array AudioDSP::get_next_chunk(int num_frames, float ratio) {
	PackedVector2Array chunk;
	if (m_samples.empty() || num_frames <= 0) {
		return chunk;
	}

	if (ratio <= 0.0f) {
		ratio = 1.0f;
	}

	m_ratio.store(ratio);

	std::vector<float> output_buffer(num_frames, 0.0f);
	size_t retrieved = 0;

	{
		std::unique_lock<std::mutex> lock(m_mutex);

		// Pop samples from the ring buffer
		retrieved = m_ring_buffer.pop(output_buffer.data(), num_frames);

		// Update playback position based on retrieved frames
		m_playback_sec += (static_cast<float>(retrieved) / 44100.0f) / ratio;
		if (m_playback_sec > m_duration) {
			m_playback_sec = m_duration;
		}

		// Notify background thread that we've read from buffer (freeing up space)
		m_cond.notify_all();
	}

	if (retrieved > 0) {
		chunk.resize(retrieved);
		Vector2* write_ptr = chunk.ptrw();
		for (size_t i = 0; i < retrieved; ++i) {
			float val = output_buffer[i];
			write_ptr[i] = Vector2(val, val);
		}
	}

	return chunk;
}

void AudioDSP::seek_pos(float position_seconds) {
	if (position_seconds < 0.0f) {
		position_seconds = 0.0f;
	}

	std::unique_lock<std::mutex> lock(m_mutex);
	m_playback_sec = position_seconds;
	m_seek_target_idx = static_cast<size_t>(position_seconds * 44100.0f);
	if (m_seek_target_idx > m_total_samples) {
		m_seek_target_idx = m_total_samples;
	}

	if (m_thread_running) {
		m_seek_pending = true;
		m_cond.notify_all();
		m_cond.wait(lock, [this]() { return !m_seek_pending; });
	} else {
		m_read_idx = m_seek_target_idx;
		if (m_stretcher) {
			m_stretcher->reset();
		}
		m_ring_buffer.clear();
		load_cache_at(m_read_idx);
	}
}

float AudioDSP::get_playback_position() const {
	std::unique_lock<std::mutex> lock(m_mutex);
	return m_playback_sec;
}

float AudioDSP::get_duration() const {
	return m_duration;
}

bool AudioDSP::is_finished() const {
	std::unique_lock<std::mutex> lock(m_mutex);
	if (m_stretcher) {
		return m_read_idx >= m_total_samples && m_stretcher->available() == 0 && m_ring_buffer.size() == 0;
	}
	return m_read_idx >= m_total_samples && m_ring_buffer.size() == 0;
}

void AudioDSP::clear_stream() {
	stop_thread();

	std::unique_lock<std::mutex> lock(m_mutex);
	if (m_stretcher) {
		delete m_stretcher;
		m_stretcher = nullptr;
	}
	m_samples.clear();
	m_read_idx = 0;
	m_duration = 0.0f;
	m_total_samples = 0;
	m_cache_start_idx = 0;
	m_playback_sec = 0.0f;
	m_ring_buffer.clear();
	m_file_path.clear();

	if (!m_temp_mono_path.empty()) {
		std::remove(m_temp_mono_path.c_str());
		m_temp_mono_path.clear();
	}
}

void AudioDSP::start_thread() {
	std::unique_lock<std::mutex> lock(m_mutex);
	if (m_thread_running) return;

	m_thread_running = true;
	m_seek_pending = false;
	m_ring_buffer.clear();
	m_thread = std::thread(&AudioDSP::thread_loop, this);
}

void AudioDSP::stop_thread() {
	{
		std::unique_lock<std::mutex> lock(m_mutex);
		if (!m_thread_running) return;
		m_thread_running = false;
		m_cond.notify_all();
	}
	if (m_thread.joinable()) {
		m_thread.join();
	}
}

void AudioDSP::thread_loop() {
	while (m_thread_running) {
		std::unique_lock<std::mutex> lock(m_mutex);

		m_cond.wait_for(lock, std::chrono::milliseconds(10), [this]() {
			return !m_thread_running ||
				   (m_ring_buffer.available_write() >= 1024 && m_read_idx < m_total_samples) ||
				   m_seek_pending;
		});

		if (!m_thread_running) {
			break;
		}

		if (m_seek_pending) {
			m_read_idx = m_seek_target_idx;
			if (m_stretcher) {
				m_stretcher->reset();
			}
			m_ring_buffer.clear();
			lock.unlock();
			load_cache_at(m_read_idx);
			lock.lock();
			m_seek_pending = false;
			m_cond.notify_all();
			continue;
		}

		if (!m_stretcher) {
			continue;
		}

		size_t write_space = m_ring_buffer.available_write();
		if (write_space < 1024) {
			continue;
		}

		float current_ratio = m_ratio.load();
		m_stretcher->setTimeRatio(current_ratio);

		if (m_stretcher->available() < 2048 && m_read_idx < m_total_samples) {
			// Check if we need to load/slide cache window
			if (m_read_idx < m_cache_start_idx ||
				m_read_idx + 1024 > m_cache_start_idx + m_samples.size() ||
				(m_cache_start_idx + m_samples.size() - m_read_idx < 44100 && m_cache_start_idx + m_samples.size() < m_total_samples)) {
				
				lock.unlock();
				load_cache_at(m_read_idx);
				lock.lock();
			}

			if (!m_samples.empty() && m_read_idx >= m_cache_start_idx && m_read_idx < m_cache_start_idx + m_samples.size()) {
				size_t cache_offset = m_read_idx - m_cache_start_idx;
				size_t available_in_cache = m_samples.size() - cache_offset;
				size_t to_feed = std::min((size_t)1024, available_in_cache);
				to_feed = std::min(to_feed, m_total_samples - m_read_idx);

				if (to_feed > 0) {
					const float* input_ptr = &m_samples[cache_offset];
					const float* input_buffers[1] = { input_ptr };
					bool is_final = (m_read_idx + to_feed >= m_total_samples);

					lock.unlock();
					m_stretcher->process(input_buffers, to_feed, is_final);
					lock.lock();

					m_read_idx += to_feed;
				}
			} else {
				m_read_idx = m_total_samples;
			}
		}

		int available = m_stretcher->available();
		if (available > 0) {
			size_t to_retrieve = std::min((size_t)available, m_ring_buffer.available_write());
			if (to_retrieve > 0) {
				std::vector<float> output_buffer(to_retrieve);
				float* output_buffers[1] = { output_buffer.data() };

				lock.unlock();
				m_stretcher->retrieve(output_buffers, to_retrieve);
				lock.lock();

				m_ring_buffer.push(output_buffer.data(), to_retrieve);
			}
		}
	}
}

std::vector<float> AudioDSP::get_samples_in_range(float start_time, float duration_sec) {
	std::vector<float> result;
	if (duration_sec <= 0.0f) {
		return result;
	}

	size_t num_samples = static_cast<size_t>(duration_sec * 44100.0f);
	result.resize(num_samples, 0.0f);

	std::unique_lock<std::mutex> lock(m_mutex);

	if (m_samples.empty()) {
		return result;
	}

	int64_t start_idx = static_cast<int64_t>(start_time * 44100.0f);
	size_t cache_size = m_samples.size();
	int64_t total_samples_limit = static_cast<int64_t>(m_total_samples);

	for (size_t i = 0; i < num_samples; ++i) {
		int64_t idx = start_idx + i;
		if (idx < 0 || idx >= total_samples_limit) {
			result[i] = 0.0f;
		} else if (idx >= static_cast<int64_t>(m_cache_start_idx) && idx < static_cast<int64_t>(m_cache_start_idx + cache_size)) {
			result[i] = m_samples[idx - m_cache_start_idx];
		} else {
			if (idx < static_cast<int64_t>(m_cache_start_idx)) {
				result[i] = m_samples[0];
			} else {
				result[i] = m_samples[cache_size - 1];
			}
		}
	}
	return result;
}

float AudioDSP::get_cross_correlation_offset(AudioDSP* other_dsp, float self_time, float other_time, float window_size_sec) {
	if (!other_dsp) {
		return 0.0f;
	}

	float max_lag_sec = 0.1f;

	float x_start = self_time - (window_size_sec / 2.0f) - max_lag_sec;
	float x_duration = window_size_sec + 2.0f * max_lag_sec;
	std::vector<float> x_44k = get_samples_in_range(x_start, x_duration);

	float y_start = other_time - (window_size_sec / 2.0f);
	std::vector<float> y_44k = other_dsp->get_samples_in_range(y_start, window_size_sec);

	if (x_44k.empty() || y_44k.empty()) {
		return 0.0f;
	}

	auto downsample_to_4000 = [](const std::vector<float>& input) {
		float ratio = 44100.0f / 4000.0f;
		int target_size = static_cast<int>(input.size() / ratio);
		std::vector<float> output(target_size, 0.0f);
		for (int i = 0; i < target_size; ++i) {
			float src_idx = i * ratio;
			int start = static_cast<int>(src_idx);
			int end = static_cast<int>(src_idx + ratio);
			end = std::min(end, static_cast<int>(input.size()));
			float sum = 0.0f;
			int count = 0;
			for (int j = start; j < end; ++j) {
				sum += input[j];
				count++;
			}
			output[i] = (count > 0) ? (sum / count) : 0.0f;
		}
		return output;
	};

	std::vector<float> x_4k = downsample_to_4000(x_44k);
	std::vector<float> y_4k = downsample_to_4000(y_44k);

	if (x_4k.empty() || y_4k.empty()) {
		return 0.0f;
	}

	int N = y_4k.size();
	int L = static_cast<int>(max_lag_sec * 4000.0f);

	if (static_cast<int>(x_4k.size()) < N + 2 * L) {
		return 0.0f;
	}

	double den_y = 0.0;
	for (int i = 0; i < N; ++i) {
		den_y += y_4k[i] * y_4k[i];
	}

	if (den_y < 1e-6) {
		return 0.0f;
	}

	// Precompute den_x sums using sliding window
	std::vector<double> den_x_vals(2 * L + 1, 0.0);
	double current_den_x = 0.0;
	for (int i = 0; i < N; ++i) {
		current_den_x += x_4k[i] * x_4k[i];
	}
	den_x_vals[0] = current_den_x;

	for (int lag = -L + 1; lag <= L; ++lag) {
		int prev_start = lag - 1 + L;
		int new_end = lag + L + N - 1;
		current_den_x -= x_4k[prev_start] * x_4k[prev_start];
		current_den_x += x_4k[new_end] * x_4k[new_end];
		den_x_vals[lag + L] = current_den_x;
	}

	double max_corr = -2.0;
	int best_lag = 0;

	for (int lag = -L; lag <= L; ++lag) {
		double num = 0.0;
		int start_x_idx = lag + L;

		for (int i = 0; i < N; ++i) {
			num += x_4k[start_x_idx + i] * y_4k[i];
		}

		double den_x = den_x_vals[start_x_idx];
		if (den_x < 1e-6) {
			continue;
		}

		double corr = num / std::sqrt(den_x * den_y);
		if (corr > max_corr) {
			max_corr = corr;
			best_lag = lag;
		}
	}

	float offset_sec = static_cast<float>(best_lag) / 4000.0f;
	return offset_sec;
}

// Three-band RMS over one output chunk.
//
// Ported verbatim from audio_manager.gd::_calculate_chunk_rms(), which ran this
// per-sample loop in GDScript on every _process for both decks. Two one-pole
// lowpasses split each channel into low / mid / high; the band values are squared,
// L/R averaged, accumulated, and reduced to an RMS per band.
//
// Coefficients and arithmetic order are kept identical to the GDScript original so
// the clipping-prevention logic downstream sees the same numbers. Accumulation is
// in double for the same reason — GDScript floats are 64-bit.
//
// Filter state persists across chunks (that is what makes these filters work), and
// lives on the instance, so each deck's AudioDSP naturally keeps its own.
Dictionary AudioDSP::calculate_chunk_rms(const PackedVector2Array &chunk) {
	Dictionary result;

	const int64_t num_frames = chunk.size();
	if (num_frames == 0) {
		result["low"] = 0.0;
		result["mid"] = 0.0;
		result["high"] = 0.0;
		return result;
	}

	// Crossover coefficients: ~low band, and the low/mid split point.
	const double LOW_COEF = 0.0344;
	const double MID_COEF = 0.363;

	// ptr() gives a direct pointer to the packed data — no per-element marshalling.
	const Vector2 *data = chunk.ptr();

	// Pull filter state into locals so the loop stays in registers.
	double low_l = m_rms_low_l;
	double low_r = m_rms_low_r;
	double mid_low_l = m_rms_mid_low_l;
	double mid_low_r = m_rms_mid_low_r;

	double sum_low_sq = 0.0;
	double sum_mid_sq = 0.0;
	double sum_high_sq = 0.0;

	for (int64_t i = 0; i < num_frames; ++i) {
		// Left channel
		const double sample_l = static_cast<double>(data[i].x);
		low_l += LOW_COEF * (sample_l - low_l);
		mid_low_l += MID_COEF * (sample_l - mid_low_l);
		const double low_l_val = low_l;
		const double mid_l_val = mid_low_l - low_l;
		const double high_l_val = sample_l - mid_low_l;

		// Right channel
		const double sample_r = static_cast<double>(data[i].y);
		low_r += LOW_COEF * (sample_r - low_r);
		mid_low_r += MID_COEF * (sample_r - mid_low_r);
		const double low_r_val = low_r;
		const double mid_r_val = mid_low_r - low_r;
		const double high_r_val = sample_r - mid_low_r;

		sum_low_sq += (low_l_val * low_l_val + low_r_val * low_r_val) * 0.5;
		sum_mid_sq += (mid_l_val * mid_l_val + mid_r_val * mid_r_val) * 0.5;
		sum_high_sq += (high_l_val * high_l_val + high_r_val * high_r_val) * 0.5;
	}

	m_rms_low_l = low_l;
	m_rms_low_r = low_r;
	m_rms_mid_low_l = mid_low_l;
	m_rms_mid_low_r = mid_low_r;

	const double inv_frames = 1.0 / static_cast<double>(num_frames);
	result["low"] = std::sqrt(sum_low_sq * inv_frames);
	result["mid"] = std::sqrt(sum_mid_sq * inv_frames);
	result["high"] = std::sqrt(sum_high_sq * inv_frames);
	return result;
}

PackedFloat32Array AudioDSP::get_waveform_peaks(int num_bins) {
	PackedFloat32Array peaks;
	peaks.resize(num_bins);
	for (int i = 0; i < num_bins; i++) {
		peaks[i] = 0.0f;
	}

	std::vector<float> samples_copy;
	{
		std::unique_lock<std::mutex> lock(m_mutex);
		if (m_samples.empty() || num_bins <= 0) {
			return peaks;
		}
		samples_copy = m_samples;
	}

	size_t total_samples = samples_copy.size();
	double samples_per_bin = static_cast<double>(total_samples) / num_bins;

	float max_overall = 0.0f;
	for (int i = 0; i < num_bins; i++) {
		size_t start_idx = static_cast<size_t>(i * samples_per_bin);
		size_t end_idx = static_cast<size_t>((i + 1) * samples_per_bin);
		if (end_idx > total_samples) {
			end_idx = total_samples;
		}
		if (start_idx >= end_idx) {
			peaks[i] = 0.0f;
			continue;
		}

		// Calculate "averaged peak amplitudes" using 256-sample sub-windows
		float sum_peaks = 0.0f;
		int count = 0;
		size_t sub_window = 256;
		for (size_t k = start_idx; k < end_idx; k += sub_window) {
			float sub_peak = 0.0f;
			size_t limit = std::min(k + sub_window, end_idx);
			for (size_t m = k; m < limit; m++) {
				float abs_val = std::abs(samples_copy[m]);
				if (abs_val > sub_peak) {
					sub_peak = abs_val;
				}
			}
			sum_peaks += sub_peak;
			count++;
		}
		float peak = (count > 0) ? (sum_peaks / count) : 0.0f;
		peaks[i] = peak;
		if (peak > max_overall) {
			max_overall = peak;
		}
	}

	// Normalize to 0.0 - 1.0
	if (max_overall > 0.0f) {
		for (int i = 0; i < num_bins; i++) {
			peaks[i] /= max_overall;
		}
	}

	return peaks;
}

#pragma pack(push, 1)
struct WavHeader {
	char riff[4] = {'R', 'I', 'F', 'F'};
	uint32_t riff_size = 0;
	char wave[4] = {'W', 'A', 'V', 'E'};
	char fmt[4] = {'f', 'm', 't', ' '};
	uint32_t fmt_size = 16;
	uint16_t audio_format = 1; // PCM
	uint16_t num_channels = 1; // Mono
	uint32_t sample_rate = 0;
	uint32_t byte_rate = 0;
	uint16_t block_align = 2;
	uint16_t bits_per_sample = 16;
	char data[4] = {'d', 'a', 't', 'a'};
	uint32_t data_size = 0;
};
#pragma pack(pop)

int AudioDSP::get_wav_channels(const std::string& filepath) {
	std::ifstream file(filepath, std::ios::binary);
	if (!file) return 0;

	char chunk_id[4];
	uint32_t chunk_size = 0;
	char format[4];

	if (!file.read(chunk_id, 4) || !file.read(reinterpret_cast<char*>(&chunk_size), 4) || !file.read(format, 4)) {
		return 0;
	}

	if (std::string(chunk_id, 4) != "RIFF" || std::string(format, 4) != "WAVE") {
		return 0;
	}

	uint32_t sample_rate = 0;
	uint32_t byte_rate = 0;

	while (file.read(chunk_id, 4) && file.read(reinterpret_cast<char*>(&chunk_size), 4)) {
		std::string id(chunk_id, 4);

		if (id == "fmt ") {
			uint16_t audio_format = 0;
			uint16_t num_channels = 0;
			uint16_t block_align = 0;
			uint16_t bits_per_sample = 0;

			if (file.read(reinterpret_cast<char*>(&audio_format), 2) &&
				file.read(reinterpret_cast<char*>(&num_channels), 2) &&
				file.read(reinterpret_cast<char*>(&sample_rate), 4) &&
				file.read(reinterpret_cast<char*>(&byte_rate), 4) &&
				file.read(reinterpret_cast<char*>(&block_align), 2) &&
				file.read(reinterpret_cast<char*>(&bits_per_sample), 2)) {
				return num_channels;
			}
			break;
		} else {
			file.seekg(chunk_size, std::ios::cur);
		}
	}
	return 0;
}

bool AudioDSP::downmix_wav_to_mono(const std::string& input_path, const std::string& output_path) {
	std::ifstream infile(input_path, std::ios::binary);
	if (!infile) return false;

	char chunk_id[4];
	uint32_t chunk_size = 0;
	char format[4];

	if (!infile.read(chunk_id, 4) || !infile.read(reinterpret_cast<char*>(&chunk_size), 4) || !infile.read(format, 4)) {
		return false;
	}

	if (std::string(chunk_id, 4) != "RIFF" || std::string(format, 4) != "WAVE") {
		return false;
	}

	uint16_t audio_format = 0;
	uint16_t num_channels = 0;
	uint32_t sample_rate = 0;
	uint32_t byte_rate = 0;
	uint16_t block_align = 0;
	uint16_t bits_per_sample = 0;
	uint32_t data_size = 0;
	std::streampos data_pos;

	while (infile.read(chunk_id, 4) && infile.read(reinterpret_cast<char*>(&chunk_size), 4)) {
		std::string id(chunk_id, 4);

		if (id == "fmt ") {
			std::streampos next_chunk = infile.tellg() + std::streamoff(chunk_size);
			if (infile.read(reinterpret_cast<char*>(&audio_format), 2) &&
				infile.read(reinterpret_cast<char*>(&num_channels), 2) &&
				infile.read(reinterpret_cast<char*>(&sample_rate), 4) &&
				infile.read(reinterpret_cast<char*>(&byte_rate), 4) &&
				infile.read(reinterpret_cast<char*>(&block_align), 2) &&
				infile.read(reinterpret_cast<char*>(&bits_per_sample), 2)) {
				
				// Handle WAVE_FORMAT_EXTENSIBLE
				if (audio_format == 0xFFFE && chunk_size >= 40) {
					uint16_t cbSize = 0;
					infile.read(reinterpret_cast<char*>(&cbSize), 2);
					uint16_t validBits = 0;
					infile.read(reinterpret_cast<char*>(&validBits), 2);
					uint32_t mask = 0;
					infile.read(reinterpret_cast<char*>(&mask), 4);
					
					char subformat_guid[16];
					infile.read(subformat_guid, 16);
					// Subformat GUID: first 2 bytes are the format code
					uint16_t sub_format = *reinterpret_cast<uint16_t*>(subformat_guid);
					audio_format = sub_format;
				}
			}
			infile.seekg(next_chunk);
		} else if (id == "data") {
			data_size = chunk_size;
			data_pos = infile.tellg();
			break;
		} else {
			infile.seekg(chunk_size, std::ios::cur);
		}
	}

	if (num_channels == 0 || sample_rate == 0 || bits_per_sample == 0 || data_size == 0) {
		return false;
	}

	infile.seekg(data_pos);

	std::ofstream outfile(output_path, std::ios::binary);
	if (!outfile) return false;

	// Write placeholder header
	WavHeader out_hdr;
	out_hdr.sample_rate = sample_rate;
	out_hdr.byte_rate = sample_rate * 2;
	outfile.write(reinterpret_cast<const char*>(&out_hdr), sizeof(out_hdr));

	size_t bytes_per_sample = bits_per_sample / 8;
	size_t frame_size = num_channels * bytes_per_sample;
	size_t num_frames = data_size / frame_size;

	std::vector<char> frame_buf(frame_size);
	uint32_t written_data_size = 0;

	for (size_t f = 0; f < num_frames; ++f) {
		if (!infile.read(frame_buf.data(), frame_size)) {
			break;
		}

		double sum = 0.0;
		for (size_t ch = 0; ch < num_channels; ++ch) {
			const char* sample_ptr = &frame_buf[ch * bytes_per_sample];
			double val = 0.0;

			if (audio_format == 1) { // PCM
				if (bits_per_sample == 8) {
					uint8_t u8 = *reinterpret_cast<const uint8_t*>(sample_ptr);
					val = (static_cast<double>(u8) - 128.0) / 128.0;
				} else if (bits_per_sample == 16) {
					int16_t s16 = *reinterpret_cast<const int16_t*>(sample_ptr);
					val = static_cast<double>(s16) / 32768.0;
				} else if (bits_per_sample == 24) {
					// 24-bit signed int (3 bytes, little endian)
					int32_t s24 = (static_cast<uint8_t>(sample_ptr[0])) |
					              (static_cast<uint8_t>(sample_ptr[1]) << 8) |
					              (static_cast<uint8_t>(sample_ptr[2]) << 16);
					if (s24 & 0x800000) {
						s24 |= 0xFF000000;
					}
					val = static_cast<double>(s24) / 8388608.0;
				} else if (bits_per_sample == 32) {
					int32_t s32 = *reinterpret_cast<const int32_t*>(sample_ptr);
					val = static_cast<double>(s32) / 2147483648.0;
				}
			} else if (audio_format == 3) { // IEEE Float
				if (bits_per_sample == 32) {
					float f32 = *reinterpret_cast<const float*>(sample_ptr);
					val = static_cast<double>(f32);
				} else if (bits_per_sample == 64) {
					double f64 = *reinterpret_cast<const double*>(sample_ptr);
					val = f64;
				}
			}
			sum += val;
		}

		double avg = sum / num_channels;
		if (avg > 1.0) avg = 1.0;
		if (avg < -1.0) avg = -1.0;

		int16_t out_sample = static_cast<int16_t>(avg * 32767.0);
		outfile.write(reinterpret_cast<const char*>(&out_sample), 2);
		written_data_size += 2;
	}

	// Update WAV header with correct sizes
	outfile.seekp(0);
	out_hdr.data_size = written_data_size;
	out_hdr.riff_size = written_data_size + 36;
	outfile.write(reinterpret_cast<const char*>(&out_hdr), sizeof(out_hdr));

	return true;
}

int AudioDSP::get_audio_channels(const std::string& filepath) {
	// Try ffprobe first as it is the most reliable for all formats
	int ch_ffprobe = get_channels_via_ffprobe(filepath);
	if (ch_ffprobe > 0) {
		return ch_ffprobe;
	}

	// First, check if WAV to avoid MetadataReader overhead for WAV
	std::string path_lower = filepath;
	std::transform(path_lower.begin(), path_lower.end(), path_lower.begin(), ::tolower);
	if (path_lower.size() >= 4 && path_lower.substr(path_lower.size() - 4) == ".wav") {
		int ch = get_wav_channels(filepath);
		if (ch > 0) return ch;
	}

	try {
		essentia::standard::Algorithm* reader = essentia::standard::AlgorithmFactory::create("MetadataReader");
		reader->configure("filename", filepath);
		reader->configure("failOnError", false);

		std::string title, artist, album, comment, genre, tracknumber, date;
		double duration = 0.0;
		int bitrate = 0, sampleRate = 0, channels = 0;
		essentia::Pool tagPool;

		reader->output("title").set(title);
		reader->output("artist").set(artist);
		reader->output("album").set(album);
		reader->output("comment").set(comment);
		reader->output("genre").set(genre);
		reader->output("tracknumber").set(tracknumber);
		reader->output("date").set(date);
		reader->output("duration").set(duration);
		reader->output("bitrate").set(bitrate);
		reader->output("sampleRate").set(sampleRate);
		reader->output("channels").set(channels);
		reader->output("tagPool").set(tagPool);

		reader->compute();
		delete reader;
		return channels;
	} catch (...) {
		return 0;
	}
}

int AudioDSP::get_channels_via_ffprobe(const std::string& filepath) {
	std::string ffprobe_bin = "ffprobe";
	
	// Check standard macOS Homebrew paths first using access()
	if (access("/opt/homebrew/bin/ffprobe", F_OK) == 0) {
		ffprobe_bin = "/opt/homebrew/bin/ffprobe";
	} else if (access("/usr/local/bin/ffprobe", F_OK) == 0) {
		ffprobe_bin = "/usr/local/bin/ffprobe";
	}

	std::string cmd = "\"" + ffprobe_bin + "\" -v error -select_streams a:0 -show_entries stream=channels -of default=noprint_wrappers=1:nokey=1 \"" + filepath + "\"";
	FILE* pipe = popen(cmd.c_str(), "r");
	if (!pipe) return 0;

	char buffer[128];
	std::string result = "";
	while (!feof(pipe)) {
		if (fgets(buffer, 128, pipe) != nullptr) {
			result += buffer;
		}
	}
	pclose(pipe);

	try {
		return std::stoi(result);
	} catch (...) {
		return 0;
	}
}

bool AudioDSP::downmix_via_ffmpeg(const std::string& input_path, const std::string& output_path) {
	std::string ffmpeg_bin = "ffmpeg";
	
	// Check standard macOS Homebrew paths first using access()
	if (access("/opt/homebrew/bin/ffmpeg", F_OK) == 0) {
		ffmpeg_bin = "/opt/homebrew/bin/ffmpeg";
	} else if (access("/usr/local/bin/ffmpeg", F_OK) == 0) {
		ffmpeg_bin = "/usr/local/bin/ffmpeg";
	}
	
	std::string cmd = "\"" + ffmpeg_bin + "\" -y -i \"" + input_path + "\" -ac 1 -ar 44100 \"" + output_path + "\" > /dev/null 2>&1";
	int ret = std::system(cmd.c_str());
	return ret == 0;
}
