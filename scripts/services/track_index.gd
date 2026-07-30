## track_index.gd
## Pure-data pipeline for the library table: build → filter → sort → group.
##
## Built once per scan; filter/sort/group are cheap array operations over the
## prebuilt rows, so the UI can re-run them per keystroke. No nodes, no I/O
## beyond reading MetadataService's in-memory cache at build time.
##
## Row schema:
##   href: String              — WebDAV path (identity)
##   title: String             — display title (never percent-encoded)
##   artist: String            — "Unknown Artist" sentinel when unknown
##   album: String             — "Unknown Album" sentinel when unknown
##   artist_source: String     — "cache" | "file" | "folder" | "unknown"
##   album_source: String      — "cache" | "file" | "folder" | "unknown"
##   genre: String             — "" when unknown
##   bpm: float                — 0.0 when unknown
##   key: String               — Camelot key, "" when unknown
##   album_art_local: String   — local image path, "" when not yet fetched
##   artist_image_local: String — local image path, "" when not yet fetched
##
## No class_name on purpose: consumers preload this script into a const, which
## resolves without depending on the editor's global-class cache.
extends RefCounted

## Group-mode identifiers, matched against toolbar OptionButton metadata.
const GROUP_NONE   := "none"
const GROUP_ARTIST := "artist"
const GROUP_ALBUM  := "album"
const GROUP_GENRE  := "genre"

## Header text for rows whose group-key field is unknown. A quiet dash, not a
## shouty "Unknown Artist" — the app is honest about ignorance without noise.
const UNKNOWN_HEADER := "—"

var rows: Array[Dictionary] = []


## Builds the index from a scanned file list. One parse_track_info + one cache
## read per file; everything downstream reuses these rows.
func build(files: Array) -> void:
	rows.clear()
	for href: String in files:
		rows.append(build_row(href))


## Re-derives one row after its metadata changed (e.g. lookup_metadata enriched
## genre/BPM during playback). Returns the fresh row, or {} if the href is not
## in the index.
func refresh(href: String) -> Dictionary:
	for i in rows.size():
		if rows[i].href == href:
			rows[i] = build_row(href)
			return rows[i]
	return {}


## Public row builder — the playlist screen uses it to derive rows for member
## hrefs without owning a full index.
func build_row(href: String) -> Dictionary:
	var row := {
		"href": href,
		"title": href.get_file().uri_decode().get_basename(),
		"artist": "Unknown Artist",
		"album": "Unknown Album",
		"artist_source": "unknown",
		"album_source": "unknown",
		"genre": "",
		"bpm": 0.0,
		"key": "",
		"year": 0,
		"album_art_local": "",
		"artist_image_local": "",
	}
	if is_instance_valid(MetadataService):
		var info: Dictionary = MetadataService.parse_track_info(href)
		if not (info.track as String).is_empty():
			row.title = info.track
		row.artist = info.artist
		row.album = info.album
		row.artist_source = info.get("artist_source", "unknown")
		row.album_source = info.get("album_source", "unknown")
		row.year = info.get("year", 0)

		var cached: Dictionary = MetadataService.get_cached_metadata(href)
		if not cached.is_empty():
			var genre: String = cached.get("genre", "")
			if not MetadataService._is_unknown_genre(genre):
				row.genre = genre
			row.bpm = cached.get("bpm", 0.0)
			row.key = cached.get("musical_key", "")
			row.album_art_local = cached.get("album_art_local", "")
			row.artist_image_local = cached.get("artist_image_local", "")
	return row


## Case-insensitive substring match across title, artist, album, and genre.
## Empty query matches everything. THE definition of a filter — smart
## playlists store a query and re-run this same predicate, so the library
## search box and a smart playlist can never disagree about membership.
static func matches_query(row: Dictionary, query: String) -> bool:
	var q := query.strip_edges().to_lower()
	if q.is_empty():
		return true
	return q in (row.title as String).to_lower() \
		or q in (row.artist as String).to_lower() \
		or q in (row.album as String).to_lower() \
		or q in (row.genre as String).to_lower()


## Does this row belong to the given entity (artist/album/genre value)? THE
## definition of dynamic-group membership — a group never stores tracks, only
## entities, and every view of a group's contents runs this same predicate
## against the live library, so two views of the same group can never disagree.
static func matches_entity(row: Dictionary, entity_type: String, value: String) -> bool:
	match entity_type:
		GROUP_ARTIST:
			return row.artist_source != "unknown" and row.artist == value
		GROUP_ALBUM:
			return row.album_source != "unknown" and row.album == value
		GROUP_GENRE:
			return not (row.genre as String).is_empty() and row.genre == value
	return false


## Does this row belong to ANY entity in a dynamic group? A group with several
## entities is their union, not their intersection — adding "Björk" and
## "Ambient" to a group means tracks matching EITHER.
static func matches_any_entity(row: Dictionary, entities: Array) -> bool:
	for e in entities:
		if matches_entity(row, e.get("type", ""), e.get("value", "")):
			return true
	return false


func filtered(query: String) -> Array[Dictionary]:
	if query.strip_edges().is_empty():
		return rows.duplicate()
	var out: Array[Dictionary] = []
	for row in rows:
		if matches_query(row, query):
			out.append(row)
	return out


## Sorts in place by any column id ("title", "artist", "album", "genre",
## "year", "bpm", "key") or "order" (manual playlist position). Rows with an
## unknown sort value always sink to the end regardless of direction — an
## unknown BPM is not "0 BPM", and pretending it sorts first would be a lie.
static func sort_rows(list: Array[Dictionary], sort_key: String, ascending: bool = true) -> void:
	match sort_key:
		"order":
			# Manual playlist order — rows carry manual_pos in manual mode.
			list.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
				return (a.get("manual_pos", 0) as int) < (b.get("manual_pos", 0) as int)
			)
		"bpm":
			list.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
				var av: float = a.bpm
				var bv: float = b.bpm
				if (av > 0.0) != (bv > 0.0):
					return av > 0.0
				if av == bv:
					return (a.title as String).to_lower() < (b.title as String).to_lower()
				return av < bv if ascending else av > bv
			)
		"year":
			list.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
				var av: int = a.get("year", 0)
				var bv: int = b.get("year", 0)
				if (av > 0) != (bv > 0):
					return av > 0
				if av == bv:
					return (a.title as String).to_lower() < (b.title as String).to_lower()
				return av < bv if ascending else av > bv
			)
		"key":
			# Camelot keys sort musically: number first, then letter (8A < 8B < 9A).
			list.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
				var ak := a.key as String
				var bk := b.key as String
				if ak.is_empty() != bk.is_empty():
					return not ak.is_empty()
				var at := _camelot_tuple(ak)
				var bt := _camelot_tuple(bk)
				if at[0] == bt[0] and at[1] == bt[1]:
					return (a.title as String).to_lower() < (b.title as String).to_lower()
				var less: bool = at[0] < bt[0] if at[0] != bt[0] else (at[1] as String) < (bt[1] as String)
				return less if ascending else not less
			)
		"artist", "album", "genre":
			list.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
				var a_known := _field_known(a, sort_key)
				var b_known := _field_known(b, sort_key)
				if a_known != b_known:
					return a_known
				var av := (a[sort_key] as String).to_lower()
				var bv := (b[sort_key] as String).to_lower()
				if av == bv:
					return (a.title as String).to_lower() < (b.title as String).to_lower()
				return av < bv if ascending else av > bv
			)
		_:
			list.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
				var av := (a.title as String).to_lower()
				var bv := (b.title as String).to_lower()
				return av < bv if ascending else av > bv
			)


static func _field_known(row: Dictionary, field: String) -> bool:
	match field:
		"artist":
			return row.artist_source != "unknown"
		"album":
			return row.album_source != "unknown"
		"genre":
			return not (row.genre as String).is_empty()
	return true


static func _camelot_tuple(k: String) -> Array:
	var i := 0
	while i < k.length() and k[i].is_valid_int():
		i += 1
	return [int(k.substr(0, i)) if i > 0 else 0, k.substr(i).to_upper()]


## Distinct known values for one entity type across the given rows, each with
## its current match count, sorted case-insensitively. Used by the dynamic-
## group entity browser to list what can be added — the unknown bucket is
## never offered, since "add the Unknown Artist" isn't a meaningful entity.
static func distinct_values(rows: Array, entity_type: String) -> Array[Dictionary]:
	var counts: Dictionary = {}
	for row in rows:
		var known := true
		var value := ""
		match entity_type:
			GROUP_ARTIST:
				known = row.artist_source != "unknown"
				value = row.artist
			GROUP_ALBUM:
				known = row.album_source != "unknown"
				value = row.album
			GROUP_GENRE:
				value = row.genre
				known = not value.is_empty()
		if known:
			counts[value] = counts.get(value, 0) + 1

	var values: Array = counts.keys()
	values.sort_custom(func(a: String, b: String) -> bool: return a.to_lower() < b.to_lower())
	var out: Array[Dictionary] = []
	for v: String in values:
		out.append({"value": v, "count": counts[v]})
	return out


## Groups a (already filtered+sorted) row list by one field. Returns
## [{header: String, rows: Array[Dictionary]}], headers sorted
## case-insensitively with the unknown bucket last. GROUP_NONE returns a
## single anonymous group.
static func grouped(list: Array[Dictionary], group_key: String) -> Array[Dictionary]:
	if group_key == GROUP_NONE:
		return [{"header": "", "rows": list}]

	var buckets: Dictionary = {}
	for row in list:
		var value := UNKNOWN_HEADER
		match group_key:
			GROUP_ARTIST:
				if row.artist_source != "unknown":
					value = row.artist
			GROUP_ALBUM:
				if row.album_source != "unknown":
					value = row.album
			GROUP_GENRE:
				if not (row.genre as String).is_empty():
					value = row.genre
		if not buckets.has(value):
			buckets[value] = []
		buckets[value].append(row)

	var headers: Array = buckets.keys()
	headers.sort_custom(func(a: String, b: String) -> bool:
		# Unknown bucket sinks to the end.
		if a == UNKNOWN_HEADER:
			return false
		if b == UNKNOWN_HEADER:
			return true
		return a.to_lower() < b.to_lower()
	)

	var out: Array[Dictionary] = []
	for header: String in headers:
		out.append({"header": header, "rows": buckets[header]})
	return out
