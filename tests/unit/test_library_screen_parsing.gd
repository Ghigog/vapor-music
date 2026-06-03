extends GutTest

const LibraryScreenScene = preload("res://scenes/screens/library/library_screen.tscn")
var library_screen: Control

func before_each() -> void:
	library_screen = LibraryScreenScene.instantiate()
	add_child_autofree(library_screen)

func test_parse_track_info_structured_three_parts() -> void:
	var info = library_screen.call("_parse_track_info", "/Music/Daft Punk - Discovery - One More Time.mp3")
	assert_eq(info.artist, "Daft Punk")
	assert_eq(info.album, "Discovery")
	assert_eq(info.track, "One More Time")

func test_parse_track_info_structured_two_parts_numeric_prefix() -> void:
	# "01 - Intro.mp3" under a single album folder "/Music/Discovery/"
	var info = library_screen.call("_parse_track_info", "/Music/Discovery/01 - Intro.mp3")
	assert_eq(info.artist, "Unknown Artist")
	assert_eq(info.album, "Discovery")
	assert_eq(info.track, "Intro")

func test_parse_track_info_structured_two_parts_alphanumeric_prefix() -> void:
	# Vinyl style "A1 - Track.mp3" under album folder
	var info = library_screen.call("_parse_track_info", "/Music/Discovery/A1 - Track.mp3")
	assert_eq(info.artist, "Unknown Artist")
	assert_eq(info.album, "Discovery")
	assert_eq(info.track, "Track")

func test_parse_track_info_structured_two_parts_with_artist_and_album_folders() -> void:
	# "01 - Intro.mp3" under "/Music/Daft Punk/Discovery/"
	var info = library_screen.call("_parse_track_info", "/Music/Daft Punk/Discovery/01 - Intro.mp3")
	assert_eq(info.artist, "Daft Punk")
	assert_eq(info.album, "Discovery")
	assert_eq(info.track, "Intro")

func test_parse_track_info_various_artists_compilation() -> void:
	# Filename structured with track number, artist, track name
	var info = library_screen.call("_parse_track_info", "/Music/Compilations/Greatest Hits/01 - Queen - Bohemian Rhapsody.mp3")
	assert_eq(info.artist, "Queen")
	assert_eq(info.album, "Greatest Hits")
	assert_eq(info.track, "Bohemian Rhapsody")

func test_parse_track_info_two_parts_non_numeric_is_artist() -> void:
	# "Artist - Track.mp3" under album folder
	var info = library_screen.call("_parse_track_info", "/Music/Discovery/Daft Punk - One More Time.mp3")
	assert_eq(info.artist, "Daft Punk")
	assert_eq(info.album, "Discovery")
	assert_eq(info.track, "One More Time")

func test_parse_track_info_single_folder_split() -> void:
	# "gorillaz - demon days" single folder with track number prefix
	var info = library_screen.call("_parse_track_info", "/Music/gorillaz - demon days/01 - Intro.mp3")
	assert_eq(info.artist, "gorillaz")
	assert_eq(info.album, "demon days")
	assert_eq(info.track, "Intro")

func test_parse_track_info_uses_metadata_cache() -> void:
	if is_instance_valid(MetadataService):
		MetadataService.cache["/Music/unknown_path/song.mp3"] = {
			"artist_name": "Resolved Artist",
			"album_name": "Resolved Album",
			"track_title": "Resolved Track"
		}
		var info = library_screen.call("_parse_track_info", "/Music/unknown_path/song.mp3")
		assert_eq(info.artist, "Resolved Artist")
		assert_eq(info.album, "Resolved Album")
		assert_eq(info.track, "Resolved Track")
		MetadataService.cache.erase("/Music/unknown_path/song.mp3")

func test_parse_track_info_unicode_dashes() -> void:
	# En-dash (–) in folder name and filename
	var info_en = library_screen.call("_parse_track_info", "/Music/Gorillaz – Demon Days (Remastered) (2019)/All Alone.mp3")
	assert_eq(info_en.artist, "Gorillaz")
	assert_eq(info_en.album, "Demon Days (Remastered) (2019)")
	assert_eq(info_en.track, "All Alone")
	
	# Em-dash (—) in filename
	var info_em = library_screen.call("_parse_track_info", "/Music/Discovery/Daft Punk — One More Time.mp3")
	assert_eq(info_em.artist, "Daft Punk")
	assert_eq(info_em.album, "Discovery")
	assert_eq(info_em.track, "One More Time")


