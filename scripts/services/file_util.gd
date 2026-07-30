extends RefCounted
## Shared atomic-write helper. A plain FileAccess.open(path, WRITE) truncates
## the destination before the new content is fully written — any reader (a
## concurrent process, or the same app on its next launch) that opens the
## file during that window sees 0 bytes / a half-written JSON blob instead of
## either the old or new content.
##
## No class_name on purpose: consumers preload this script into a const,
## which resolves without depending on the editor's global-class cache (see
## track_index.gd for the same convention and the parse-error cost of not
## following it).

## Writes `content` to `path` by writing a sibling temp file first, then
## replacing `path` with a single OS-level rename. A rename is atomic on the
## filesystems this app targets, so a reader always sees either the complete
## old content or the complete new content, never a truncated one.
static func write_string_atomic(path: String, content: String) -> Error:
	var tmp_path := path + ".tmp"
	var file := FileAccess.open(tmp_path, FileAccess.WRITE)
	if file == null:
		return FileAccess.get_open_error()
	file.store_string(content)
	file.close()
	return DirAccess.rename_absolute(tmp_path, path)
