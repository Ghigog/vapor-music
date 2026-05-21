extends SceneTree

func _init() -> void:
	print("Starting raw TLS test...")
	test_raw_tls()

func parse_url(url: String) -> Dictionary:
	var result = {"protocol": "http", "host": "", "port": 80, "path": "/"}
	var parts = url.split("://", true, 1)
	if parts.size() == 2:
		result.protocol = parts[0].to_lower()
		url = parts[1]
	else:
		result.protocol = "http"
	
	if result.protocol == "https":
		result.port = 443
		
	var first_slash = url.find("/")
	var host_part = ""
	if first_slash == -1:
		host_part = url
		result.path = "/"
	else:
		host_part = url.substr(0, first_slash)
		result.path = url.substr(first_slash)
		
	if host_part.contains(":"):
		var host_port = host_part.split(":", true, 1)
		result.host = host_port[0]
		result.port = host_port[1].to_int()
	else:
		result.host = host_part
		
	return result

func test_raw_tls() -> void:
	await process_frame
	print("Engine subsystems initialized. Setting up TCP...")
	
	var url = "https://app.koofr.net/dav/Koofr"
	var url_parts = parse_url(url)
	
	var tcp = StreamPeerTCP.new()
	var err = tcp.connect_to_host(url_parts.host, url_parts.port)
	if err != OK:
		print("TCP connect failed: ", err)
		quit(1)
		return
		
	var start_time = Time.get_ticks_msec()
	while tcp.get_status() == StreamPeerTCP.STATUS_CONNECTING:
		tcp.poll()
		OS.delay_msec(10)
		if Time.get_ticks_msec() - start_time > 5000:
			print("TCP timeout")
			quit(1)
			return
			
	if tcp.get_status() != StreamPeerTCP.STATUS_CONNECTED:
		print("TCP connect status failed: ", tcp.get_status())
		quit(1)
		return
		
	print("TCP connected. Upgrading to TLS...")
	var tls = StreamPeerTLS.new()
	err = tls.connect_to_stream(tcp, url_parts.host, TLSOptions.client())
	if err != OK:
		print("TLS connect failed: ", err)
		quit(1)
		return
		
	start_time = Time.get_ticks_msec()
	while true:
		tls.poll()
		var status = tls.get_status()
		if status == StreamPeerTLS.STATUS_CONNECTED:
			break
		elif status == StreamPeerTLS.STATUS_ERROR or status == StreamPeerTLS.STATUS_DISCONNECTED:
			print("TLS handshake status failed: ", status)
			quit(1)
			return
		OS.delay_msec(10)
		if Time.get_ticks_msec() - start_time > 5000:
			print("TLS timeout")
			quit(1)
			return
			
	print("TLS connected! Sending PROPFIND request...")
	
	var headers = [
		"Depth: 1",
		"Content-Type: application/xml; charset=utf-8"
	]
	var body = '<?xml version="1.0" encoding="utf-8" ?><d:propfind xmlns:d="DAV:"><d:prop><d:displayname/><d:getcontentlength/></d:prop></d:propfind>'
	
	var req = "PROPFIND /dav/Koofr HTTP/1.1\r\n"
	req += "Host: app.koofr.net\r\n"
	for h in headers:
		req += h + "\r\n"
	req += "Content-Length: %d\r\n" % body.to_utf8_buffer().size()
	req += "Connection: close\r\n"
	req += "\r\n"
	req += body
	
	err = tls.put_data(req.to_utf8_buffer())
	if err != OK:
		print("Send failed: ", err)
		quit(1)
		return
		
	print("Request sent. Reading response...")
	var response_bytes = PackedByteArray()
	start_time = Time.get_ticks_msec()
	var done = false
	
	while not done:
		tls.poll()
		var status = tls.get_status()
		if status == StreamPeerTLS.STATUS_CONNECTED:
			var available = tls.get_available_bytes()
			if available > 0:
				var chunk = tls.get_data(available)
				if chunk[0] == OK:
					response_bytes.append_array(chunk[1])
					start_time = Time.get_ticks_msec()
				else:
					print("Read error")
					done = true
			else:
				# Keep waiting for data
				pass
		else:
			# Not connected anymore, check if we got remaining bytes or just stop
			done = true
			
		if Time.get_ticks_msec() - start_time > 10000:
			print("Read timeout")
			break
			
		if not done:
			OS.delay_msec(10)
			
	print("Read complete. Total bytes: ", response_bytes.size())
	tcp.disconnect_from_host()
	
	if response_bytes.size() == 0:
		print("No response")
		quit(1)
		return
		
	# Parse headers
	var body_start_idx = -1
	for i in range(response_bytes.size() - 3):
		if response_bytes[i] == 13 and response_bytes[i+1] == 10 and response_bytes[i+2] == 13 and response_bytes[i+3] == 10:
			body_start_idx = i + 4
			break
			
	if body_start_idx == -1:
		print("Delimiter not found")
		quit(1)
		return
		
	var header_str = response_bytes.slice(0, body_start_idx).get_string_from_utf8()
	var body_str = response_bytes.slice(body_start_idx).get_string_from_utf8()
	
	print("Headers:\n", header_str)
	print("Body snippet:\n", body_str.substr(0, min(body_str.length(), 200)))
	
	quit(0)
