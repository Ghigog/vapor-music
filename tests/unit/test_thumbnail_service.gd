## test_thumbnail_service.gd
## GUT unit tests for ThumbnailService's background decode/cache pipeline and
## its circle/rounded-mask helpers.
##
## One shared service instance for the whole file (before_all/after_all,
## like test_playlist_folder_service.gd's split) rather than one per test —
## each instance spins up a real OS thread, and the real library scan +
## AudioAnalyzer + MetadataService's own background work are already running
## concurrently during a headless test run, so minimizing how many extra
## threads this file adds keeps unrelated threaded tests from destabilizing.
extends GutTest

const TEST_DIR := "user://test_thumbnail_service/"

var service: Node

func before_all() -> void:
	service = load("res://scripts/services/thumbnail_service.gd").new()
	add_child(service)
	if not DirAccess.dir_exists_absolute(TEST_DIR):
		DirAccess.make_dir_recursive_absolute(TEST_DIR)

func after_all() -> void:
	if is_instance_valid(service):
		service.queue_free()

	var dir := DirAccess.open(TEST_DIR)
	if dir:
		dir.list_dir_begin()
		var f := dir.get_next()
		while f != "":
			DirAccess.remove_absolute(TEST_DIR + f)
			f = dir.get_next()
		dir.list_dir_end()
	if DirAccess.dir_exists_absolute(TEST_DIR):
		DirAccess.remove_absolute(TEST_DIR)

func _make_test_image(path: String, size: int = 16) -> void:
	var img := Image.create(size, size, false, Image.FORMAT_RGB8)
	img.fill(Color.RED)
	img.save_png(path)

## Polls process frames until `box` gains a "done" key or the frame budget
## runs out — the decode happens on a background thread and lands back on
## the main thread via call_deferred, so there's no signal to await directly.
func _await_result(box: Dictionary, max_frames: int = 120) -> void:
	var frames := 0
	while not box.has("done") and frames < max_frames:
		await get_tree().process_frame
		frames += 1

func test_request_empty_path_calls_back_with_null_immediately() -> void:
	var result: Dictionary = {}
	service.request("", func(tex): result["tex"] = tex; result["done"] = true)

	assert_true(result.has("done"), "Empty path should resolve synchronously, never queue")
	assert_null(result.get("tex"))

func test_request_missing_file_resolves_null() -> void:
	var result: Dictionary = {}
	service.request(TEST_DIR + "does_not_exist.png", func(tex): result["tex"] = tex; result["done"] = true)
	await _await_result(result)

	assert_true(result.has("done"), "Decode should have completed even though the file is missing")
	assert_null(result.get("tex"))

func test_request_decodes_and_caches() -> void:
	var path := TEST_DIR + "sample.png"
	_make_test_image(path)

	var first: Dictionary = {}
	service.request(path, func(tex): first["tex"] = tex; first["done"] = true, 64)
	await _await_result(first)

	assert_true(first.has("done"))
	assert_not_null(first.get("tex"), "A real image should decode to a texture")

	# Same path+size on the second call should be a cache hit — resolves
	# inline, no round trip through the background thread.
	var second: Dictionary = {}
	service.request(path, func(tex): second["tex"] = tex; second["done"] = true, 64)

	assert_true(second.has("done"), "Cache hit should resolve inline, not need a frame wait")
	assert_eq(second.get("tex"), first.get("tex"), "Same cache key should return the same texture instance")

func test_request_coalesces_concurrent_callers_for_same_key() -> void:
	var path := TEST_DIR + "sample2.png"
	_make_test_image(path)

	var a: Dictionary = {}
	var b: Dictionary = {}
	service.request(path, func(tex): a["tex"] = tex; a["done"] = true, 64)
	service.request(path, func(tex): b["tex"] = tex; b["done"] = true, 64)
	await _await_result(a)
	await _await_result(b)

	assert_true(a.has("done"))
	assert_true(b.has("done"))
	assert_eq(a.get("tex"), b.get("tex"), "Both callers waiting on the same in-flight decode should get the same texture")

func test_apply_circle_mask_sets_shader_material() -> void:
	var rect := TextureRect.new()
	rect.custom_minimum_size = Vector2(32, 32)
	ThumbnailService.apply_circle_mask(rect)

	assert_true(rect.material is ShaderMaterial)
	var mat := rect.material as ShaderMaterial
	assert_eq(mat.get_shader_parameter("corner_radius"), 9999.0)
	assert_eq(mat.get_shader_parameter("size_px"), Vector2(32, 32))
	rect.free()

func test_apply_rounded_mask_uses_given_radius() -> void:
	var rect := TextureRect.new()
	rect.custom_minimum_size = Vector2(20, 20)
	ThumbnailService.apply_rounded_mask(rect, 6.0)

	var mat := rect.material as ShaderMaterial
	assert_eq(mat.get_shader_parameter("corner_radius"), 6.0)
	rect.free()
