## test_webdav_service.gd
## GUT unit tests for WebDAV URL parsing, normalization, and XML extraction logic.
extends GutTest

var service: Node

func before_each() -> void:
	# Instantiate our WebDAVService script for isolated utility testing
	var script = load("res://scripts/services/webdav_service.gd")
	service = script.new()
	add_child_autofree(service)

func test_parse_url_standard() -> void:
	var result: Dictionary = service._parse_url("https://example.com/dav/Koofr")
	assert_eq(result.protocol, "https")
	assert_eq(result.host, "example.com")
	assert_eq(result.port, 443)
	assert_eq(result.path, "/dav/Koofr")

func test_parse_url_custom_port() -> void:
	var result: Dictionary = service._parse_url("http://192.168.1.50:8080/music/")
	assert_eq(result.protocol, "http")
	assert_eq(result.host, "192.168.1.50")
	assert_eq(result.port, 8080)
	assert_eq(result.path, "/music/")

func test_normalize_path() -> void:
	assert_eq(service._normalize_path("Music"), "/Music/")
	assert_eq(service._normalize_path("/Music/"), "/Music/")
	assert_eq(service._normalize_path("http://host/dav/Music"), "/dav/Music/")
	assert_eq(service._normalize_path(""), "/")

func test_discover_folders_from_xml() -> void:
	var xml = """<?xml version="1.0" encoding="utf-8" ?>
<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/dav/Koofr/Music/</d:href>
    <d:propstat>
      <d:prop>
        <d:resourcetype><d:collection/></d:resourcetype>
      </d:prop>
    </d:propstat>
  </d:response>
  <d:response>
    <d:href>/dav/Koofr/Music/Synthwave/</d:href>
    <d:propstat>
      <d:prop>
        <d:resourcetype><d:collection/></d:resourcetype>
      </d:prop>
    </d:propstat>
  </d:response>
</d:multistatus>"""

	var folders: Array = service._discover_folders_from_xml(xml)
	assert_eq(folders.size(), 2)
	assert_true(folders.has("/dav/Koofr/Music/"))
	assert_true(folders.has("/dav/Koofr/Music/Synthwave/"))

func test_parse_webdav_xml_files() -> void:
	var xml = """<?xml version="1.0" encoding="utf-8" ?>
<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/dav/Koofr/Music/song1.mp3</d:href>
  </d:response>
  <d:response>
    <d:href>/dav/Koofr/Music/album_art.jpg</d:href>
  </d:response>
  <d:response>
    <d:href>/dav/Koofr/Music/song2.FLAC</d:href>
  </d:response>
</d:multistatus>"""

	var tracks: Array = service._parse_webdav_xml(xml)
	assert_eq(tracks.size(), 2)
	assert_true(tracks.has("/dav/Koofr/Music/song1.mp3"))
	assert_true(tracks.has("/dav/Koofr/Music/song2.FLAC"))
