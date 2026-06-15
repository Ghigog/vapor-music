extends Object
class_name AutoEQParser

## Parses a raw AutoEQ parametric equalizer profile text and converts it
## into structured Godot data. Returns a Dictionary containing:
## - "preamp": float (clamped between -60.0 and 24.0, automatically adjusted to prevent clipping)
## - "bands": Array of Dictionaries with "frequency", "gain", "q", "type", and "enabled"
static func parse_profile_text(text: String) -> Dictionary:
	var parsed_preamp: float = 0.0
	var has_preamp: bool = false
	var bands: Array = []

	var preamp_regex = RegEx.new()
	preamp_regex.compile("(?i)Preamp:\\s*([+-]?\\d+(?:\\.\\d+)?)\\s*(?:dB)?")

	var filter_regex = RegEx.new()
	# Matches: (Filter 1:)? (ON|OFF)? (PK|LSC|HSC|NOT|LP|HP) Fc (freq) Hz Gain (gain) dB Q (q)
	# Handles optional spaces and suffixes like 'Hz' or 'dB'
	filter_regex.compile("(?i)(?:Filter\\s*\\d*:\\s*)?(?:(ON|OFF)\\s+)?(PK|LSC|HSC|NOT|LP|HP)\\s+Fc\\s*([+-]?\\d+(?:\\.\\d+)?)\\s*(?:Hz)?\\s*Gain\\s*([+-]?\\d+(?:\\.\\d+)?)\\s*(?:dB)?\\s*Q\\s*([+-]?\\d+(?:\\.\\d+)?)")

	var lines = text.split("\n")
	for line in lines:
		line = line.strip_edges()
		if line.is_empty() or line.begins_with("#"):
			continue

		var preamp_match = preamp_regex.search(line)
		if preamp_match:
			parsed_preamp = preamp_match.get_string(1).to_float()
			has_preamp = true
			continue

		var filter_match = filter_regex.search(line)
		if filter_match:
			var status_str = filter_match.get_string(1)
			var type_str = filter_match.get_string(2)
			var freq_str = filter_match.get_string(3)
			var gain_str = filter_match.get_string(4)
			var q_str = filter_match.get_string(5)

			var enabled = true
			if not status_str.is_empty() and status_str.to_upper() == "OFF":
				enabled = false

			var freq = freq_str.to_float()
			var gain = gain_str.to_float()
			var q = q_str.to_float()

			# Value validation and clamping
			freq = clamp(freq, 20.0, 22000.0)
			q = clamp(q, 0.01, 10.0)
			gain = clamp(gain, -24.0, 24.0)

			var mapped_type = "peaking"
			match type_str.to_upper():
				"PK", "PEAKING":
					mapped_type = "peaking"
				"LSC", "LOW_SHELF":
					mapped_type = "low_shelf"
				"HSC", "HIGH_SHELF":
					mapped_type = "high_shelf"
				"NOT", "NOTCH":
					mapped_type = "notch"
				"LP", "LOW_PASS":
					mapped_type = "low_pass"
				"HP", "HIGH_PASS":
					mapped_type = "high_pass"

			bands.append({
				"frequency": freq,
				"gain": gain,
				"q": q,
				"type": mapped_type,
				"enabled": enabled
			})

	# Calculate headroom required to prevent clipping
	var max_boost: float = 0.0
	for band in bands:
		if band["enabled"] and band["gain"] > 0.0:
			if band["gain"] > max_boost:
				max_boost = band["gain"]

	var safe_preamp = -max_boost
	var final_preamp = parsed_preamp
	if has_preamp:
		final_preamp = min(parsed_preamp, safe_preamp)
	else:
		final_preamp = safe_preamp

	# Clamp preamp to Godot's supported range [-60.0, 24.0]
	final_preamp = clamp(final_preamp, -60.0, 24.0)

	return {
		"preamp": final_preamp,
		"bands": bands
	}


## Reads a file from local storage/resources and parses its content
static func parse_profile_file(file_path: String) -> Dictionary:
	if not FileAccess.file_exists(file_path):
		push_error("AutoEQParser: Profile file not found: " + file_path)
		return {
			"preamp": 0.0,
			"bands": []
		}

	var file = FileAccess.open(file_path, FileAccess.READ)
	if not file:
		push_error("AutoEQParser: Failed to open profile file: " + file_path)
		return {
			"preamp": 0.0,
			"bands": []
		}

	var content = file.get_as_text()
	file.close()
	return parse_profile_text(content)
