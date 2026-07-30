extends Node
## ThumbnailService
##
## Decodes local image files off the main thread and caches the resulting
## ImageTexture, keyed by path+size. Callers never pay for a synchronous
## Image.load_from_file — a cache hit resolves immediately, a miss enqueues a
## background decode and calls back later. Mirrors AudioAnalyzer's
## Thread/Mutex/Semaphore worker pattern, the only other background-thread
## precedent in this codebase.
##
## A pool of worker threads (not just one) shares the same queue: the library
## table isn't virtualized yet (see track_table.gd), so opening a large
## library fires off a request for every row's art at once — hundreds of
## unique files. A single decoder drains that backlog serially, so whatever
## row the user happens to be looking at is a coin flip on whether its art
## has been reached yet. Multiple workers drain the same burst in parallel.

## One thread per core is wasteful contention on the shared mutex for what's
## just JPEG/PNG decode + resize; this caps it while still cutting the
## worst-case serial backlog several times over.
const WORKER_COUNT := 4

var _cache: Dictionary = {}          # "<path>@<max_size>" -> ImageTexture
var _pending: Dictionary = {}        # same key -> Array[Callable], main-thread only
var _queued_keys: Dictionary = {}    # same key -> true, guards against duplicate queue entries
var _queue: Array[Dictionary] = []   # {path, max_size, key}, shared with worker threads

var _threads: Array[Thread] = []
var _mutex: Mutex
var _semaphore: Semaphore
var _exit_thread := false

static var _mask_shader: Shader = null


func _ready() -> void:
	_mutex = Mutex.new()
	_semaphore = Semaphore.new()
	for i in WORKER_COUNT:
		var thread := Thread.new()
		thread.start(_thread_worker)
		_threads.append(thread)


func _exit_tree() -> void:
	_mutex.lock()
	_exit_thread = true
	_mutex.unlock()
	for i in _threads.size():
		_semaphore.post()
	for thread in _threads:
		thread.wait_to_finish()


## Resolves a decoded, capped-size ImageTexture for `path`. Calls `callback`
## with the texture (or null for an empty path / failed decode). Always safe
## to call from a synchronous row-building loop: a cache hit or empty path
## calls back inline, a miss enqueues a background decode and returns
## immediately — the callback fires later via call_deferred.
func request(path: String, callback: Callable, max_size: int = 128) -> void:
	if path.is_empty():
		callback.call(null)
		return

	var key := "%s@%d" % [path, max_size]
	if _cache.has(key):
		callback.call(_cache[key])
		return

	if _pending.has(key):
		(_pending[key] as Array).append(callback)
		return

	_pending[key] = [callback]

	_mutex.lock()
	var enqueued := false
	if not _queued_keys.has(key):
		_queued_keys[key] = true
		_queue.append({"path": path, "max_size": max_size, "key": key})
		enqueued = true
	_mutex.unlock()
	if enqueued:
		_semaphore.post()


func _thread_worker() -> void:
	while true:
		_semaphore.wait()

		_mutex.lock()
		if _exit_thread:
			_mutex.unlock()
			break
		if _queue.is_empty():
			_mutex.unlock()
			continue
		var job: Dictionary = _queue.pop_front()
		_queued_keys.erase(job.key)
		_mutex.unlock()

		var img: Image = null
		if FileAccess.file_exists(job.path):
			img = Image.load_from_file(job.path)
			if img and (img.get_width() > job.max_size or img.get_height() > job.max_size):
				img.resize(job.max_size, job.max_size, Image.INTERPOLATE_LANCZOS)

		call_deferred("_on_decoded", job.key, img)


func _on_decoded(key: String, img: Image) -> void:
	var tex: ImageTexture = null
	if img:
		tex = ImageTexture.create_from_image(img)
		_cache[key] = tex

	var callbacks: Array = _pending.get(key, [])
	_pending.erase(key)
	for cb: Callable in callbacks:
		# A callback's bound target (e.g. a screen or row control) can be
		# freed between request() and the decode landing back on the main
		# thread; calling it then throws "on a null instance" instead of
		# just being a no-op.
		if cb.is_valid():
			cb.call(tex)


## Masks `rect`'s own texture content to a circle. Reads size_px from
## `rect.custom_minimum_size`, so call this after sizing the rect.
static func apply_circle_mask(rect: TextureRect) -> void:
	_apply_mask(rect, 9999.0)


## Masks `rect`'s own texture content to a rounded-rect with the given corner
## radius in pixels.
static func apply_rounded_mask(rect: TextureRect, radius: float) -> void:
	_apply_mask(rect, radius)


static func _apply_mask(rect: TextureRect, radius: float) -> void:
	if _mask_shader == null:
		_mask_shader = load("res://assets/shaders/image_mask.gdshader") as Shader
	var mat := ShaderMaterial.new()
	mat.shader = _mask_shader
	mat.set_shader_parameter("size_px", rect.custom_minimum_size)
	mat.set_shader_parameter("corner_radius", radius)
	rect.material = mat
