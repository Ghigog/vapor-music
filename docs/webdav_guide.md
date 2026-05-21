# WebDAV Music Vault Setup Guide

Setting up WebDAV is an excellent choice for a minimalist, local-first app because it places the user completely in control of their files without corporate lock-in.

Here is the exact step-by-step guide to setting up your own WebDAV server with your music library, followed by a developer documentation blueprint showing how to implement this protocol cleanly inside your Godot-based client app.

---

## Part 1: Setting up Your Free WebDAV Music Vault

To test your engine and build the app, we will use **Koofr** because it is a privacy-first, EU-based independent cloud that provides 2GB for free and fully supports standard WebDAV out of the box.

### Step 1: Create Your Account

1. Go to [koofr.eu](https://koofr.eu/) and sign up for a free account.
2. Verify your email and log in to the web interface.

### Step 2: Structure Your Music Library

1. In the Koofr dashboard, create a brand new folder at the root level called Vapor Music.
2. Open that folder and drop in a couple of high-quality music files (FLAC or MP3) to test your streaming and wavelength analysis engine later.

### Step 3: Generate the Security Credentials

WebDAV protocols require a separate "App Password" so your master login password is never exposed to external clients.

1. Click your **Profile Icon** in the top right corner of Koofr and select **Account**.
2. Navigate to the **Preferences** tab on the left menu, then click **Security**.
3. Under the *App Passwords* section, enter a name for your application (e.g., Vapor Music App) and click **Generate**.
4. **Copy the generated password immediately.** It will look like a long string of random characters.

### Step 4: Keep Note of Your Connection Details

You now have the exact coordinates needed to link your player app to your music:

* **Base URL:** [https://app.koofr.net/dav/Koofr](https://app.koofr.net/dav/Koofr)
* **Username:** Your Koofr registration email address.
* **Password:** The unique App Password you just generated.

---

## Part 2: Technical Integration Documentation for Vapor Music

The following technical documentation outlines how the Godot client handles directory mapping, XML parsing, and zero-latency audio streaming via WebDAV extensions using raw HTTP methods.

### 1. Architectural Overview

Vapor Music communicates with remote cloud servers using the standard WebDAV extension layer of the **HTTP/1.1** protocol. Because Godot's built-in HTTPRequest node handles standard HTTP verbs natively, the app can run asynchronous network tasks without needing complex third-party binary GDExtensions.

```
┌──────────────────┐               PROPFIND (XML)              ┌──────────────────┐
│   Godot Engine   │ ────────────────────────────────────────> │  WebDAV Server   │
│  (Client App)    │ <──────────────────────────────────────── │ (Koofr/OwnCloud) │
└──────────────────┘          207 Multi-Status Response        └──────────────────┘

```

### 2. Network Implementation Blueprint

To map out a user's remote files, the app uses the PROPFIND method. WebDAV servers reply to this request with an XML file containing structural details of everything inside that directory.

Here is the modular script pattern to integrate directly into your Godot project core:

```gdscript
# webdav_service.gd
extends Node
class_name WebDAVService

signal library_scanned(files: Array)

var base_url: String = ""
var username: String = ""
var app_password: String = ""

# Generates the required Base64 header for private cloud authentication
func get_auth_header() -> String:
	var auth_str = username + ":" + app_password
	var base64_auth = Marshalls.utf8_to_base64(auth_str)
	return "Authorization: Basic " + base64_auth

# Scans the root music directory on the server
func scan_music_directory(target_folder: String = "Vapor Music") -> void:
	var http = HTTPRequest.new()
	add_child(http)
	http.request_completed.connect(self._on_propfind_completed.bind(http))
	
	# WebDAV requires Depth: 1 to look exactly inside the directory folder
	var headers = PackedStringArray([
		get_auth_header(),
		"Depth: 1",
		"Content-Type: application/xml; charset=utf-8"
	])
	
	# The body requests the file display name and content-length for streaming audio
	var xml_query = '<?xml version="1.0" encoding="utf-8" ?>' + \
					'<d:propfind xmlns:d="DAV:">' + \
					'<d:prop><d:displayname/><d:getcontentlength/></d:prop>' + \
					'</d:propfind>'
					
	var full_url = base_url.plus_file(target_folder)
	
	# We use HTTPClient.METHOD_CUSTOM because PROPFIND is a WebDAV extension
	var err = http.request(full_url, headers, HTTPClient.METHOD_CUSTOM, xml_query)
	if err != OK:
		push_error("Failed to initiate WebDAV request.")

func _on_propfind_completed(result: int, response_code: int, headers: PackedStringArray, body: PackedByteArray, http_node: HTTPRequest) -> void:
	http_node.queue_free() # Clean up node memory instantly
	
	if response_code != 207: # 207 Multi-Status is the correct response code for WebDAV
		push_error("Server rejected WebDAV request with code: %d" % response_code)
		return
		
	var xml_string = body.get_string_from_utf8()
	var audio_files = _parse_webdav_xml(xml_string)
	library_scanned.emit(audio_files)

# Parses file nodes out of the WebDAV Multi-Status XML string
func _parse_webdav_xml(xml_content: String) -> Array:
	var tracked_files = []
	var parser = XMLParser.new()
	parser.open_buffer(xml_content.to_utf8_buffer())
	
	var current_href = ""
	
	while parser.read() == OK:
		if parser.get_node_type() == XMLParser.NODE_ELEMENT:
			var node_name = parser.get_node_name()
			if node_name == "d:href" or node_name == "D:href" or node_name == "href":
				parser.read()
				current_href = parser.get_node_data().strip_edges()
				# Filters out directories, adding audio files directly to the queue
				if current_href.ends_with(".mp3") or current_href.ends_with(".flac"):
					tracked_files.append(current_href)
	return tracked_files

```

### 3. Audiophile Audio Streaming Mechanics

To keep the application fast and avoid long download screen delays, **do not download full audio files onto the user's phone.**

1. **Leverage Godot's built-in HTTPClient chunk buffering:** When a user selects a track from the library view, spawn an individual HTTPClient session.
2. **Partial Streaming Requests:** Utilize standard HTTP Range headers (Range: bytes=0-2048000) to pull just the initial chunks of the file.
3. **Dynamic Playback Injection:** Feed the incoming byte array arrays straight into a custom Godot AudioStreamGeneratorPlayback object buffer. This initializes track playback instantly while downloading the rest of the stream smoothly in the background.

This process gives your player an immediate, seamless response time that rivals heavy commercial streaming clients, all while relying on fully independent cloud storage.
