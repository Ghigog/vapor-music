extends Button
## vibe_card.gd
## Controller for individual compatible track cards.

@onready var art_rect: TextureRect = $Margin/VBox/ArtRatio/ArtRect
@onready var placeholder_panel: Panel = $Margin/VBox/ArtRatio/PlaceholderPanel
@onready var placeholder_icon: Label = $Margin/VBox/ArtRatio/PlaceholderPanel/IconLabel
@onready var border_panel: Panel = $Margin/VBox/ArtRatio/BorderPanel
@onready var overlay_margin: MarginContainer = $Margin/VBox/ArtRatio/OverlayMargin

@onready var title_label: Label = $Margin/VBox/TextVBox/TitleLabel
@onready var artist_label: Label = $Margin/VBox/TextVBox/ArtistLabel

@onready var key_badge: Label = $Margin/VBox/TextVBox/Row1/KeyBadge
@onready var bpm_label: Label = $Margin/VBox/TextVBox/Row1/BpmLabel

@onready var type_label: Label = $Margin/VBox/TextVBox/Row2/TypeLabel
@onready var trans_label: Label = $Margin/VBox/TextVBox/Row2/TransLabel

@onready var genre_label: Label = $Margin/VBox/TextVBox/Row3/GenreLabel
@onready var div_label: Label = $Margin/VBox/TextVBox/Row3/DivLabel
@onready var energy_label: Label = $Margin/VBox/TextVBox/Row3/EnergyLabel

var track_href: String = ""

func setup(href: String, meta: Dictionary, cost: float, type: String, is_selected: bool, is_ai_preferred: bool, current_bpm: float, predicted_transition: String) -> void:
	track_href = href
	
	var info = MetadataService.parse_track_info(href)
	var theme = ThemeManager.current_theme
	
	# Clean any old badges in overlay
	for child in overlay_margin.get_children():
		child.queue_free()
	
	# Load image
	var img_path = meta.get("album_art_local", "")
	if img_path.is_empty():
		img_path = meta.get("artist_image_local", "")
		
	var has_img = false
	if not img_path.is_empty() and FileAccess.file_exists(img_path):
		var img = Image.load_from_file(img_path)
		if img:
			art_rect.texture = ImageTexture.create_from_image(img)
			has_img = true
			
	if not has_img:
		art_rect.texture = null
		placeholder_panel.visible = true
		border_panel.visible = false
		
		var sb_place = StyleBoxFlat.new()
		sb_place.bg_color = theme.GLASS_TINT
		sb_place.border_color = theme.GLASS_BORDER_SUBTLE
		sb_place.set_border_width_all(1)
		sb_place.set_corner_radius_all(theme.RADIUS_MD)
		placeholder_panel.add_theme_stylebox_override("panel", sb_place)
		
		placeholder_icon.add_theme_font_override("font", theme.font_display)
		placeholder_icon.add_theme_font_size_override("font_size", 32)
		placeholder_icon.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
	else:
		placeholder_panel.visible = false
		border_panel.visible = true
		
		var border_sb = StyleBoxFlat.new()
		border_sb.bg_color = Color(0,0,0,0)
		border_sb.border_color = theme.GLASS_BORDER_SUBTLE
		border_sb.set_border_width_all(1)
		border_sb.set_corner_radius_all(theme.RADIUS_SM)
		border_panel.add_theme_stylebox_override("panel", border_sb)
		
	# Titles
	title_label.text = info.track
	title_label.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
	title_label.add_theme_font_override("font", theme.font_display)
	title_label.add_theme_font_size_override("font_size", theme.TYPE_SM)
	
	artist_label.text = info.artist
	artist_label.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	artist_label.add_theme_font_override("font", theme.font_ui)
	artist_label.add_theme_font_size_override("font_size", theme.TYPE_XS)
	
	# Key Badge
	var key = meta.get("musical_key", "--")
	key_badge.text = " " + key + " "
	key_badge.add_theme_color_override("font_color", theme.ACCENT_CORE)
	key_badge.add_theme_font_override("font", theme.font_mono)
	key_badge.add_theme_font_size_override("font_size", theme.TYPE_2XS)
	var ksb = StyleBoxFlat.new()
	ksb.bg_color = theme.GLASS_TINT
	ksb.border_color = theme.GLASS_BORDER_SUBTLE
	ksb.set_border_width_all(1)
	ksb.set_corner_radius_all(theme.RADIUS_XS)
	key_badge.add_theme_stylebox_override("normal", ksb)
	
	# BPM Badge (including change)
	var bpm = int(meta.get("bpm", 120.0))
	var bpm_change_val = 0.0
	if current_bpm > 0.0:
		bpm_change_val = bpm - current_bpm
		
	var bpm_text = "%d BPM" % bpm
	if bpm_change_val != 0.0:
		bpm_text += " (%+d)" % int(bpm_change_val)
		
	bpm_label.text = bpm_text
	bpm_label.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	bpm_label.add_theme_font_override("font", theme.font_mono)
	bpm_label.add_theme_font_size_override("font_size", theme.TYPE_2XS)
	
	# Match type
	if type == "perfect":
		type_label.text = " Match "
		type_label.add_theme_color_override("font_color", theme.SEMANTIC_SUCCESS)
	elif type == "interesting":
		type_label.text = " Fresh "
		type_label.add_theme_color_override("font_color", theme.AQUA_CORE)
	elif type == "creative":
		type_label.text = " Switch "
		type_label.add_theme_color_override("font_color", theme.SEMANTIC_WARNING)
		
	type_label.add_theme_font_override("font", theme.font_ui)
	type_label.add_theme_font_size_override("font_size", theme.TYPE_2XS)
	var tsb = StyleBoxFlat.new()
	tsb.bg_color = Color(0,0,0,0)
	tsb.border_color = type_label.get_theme_color("font_color")
	tsb.set_border_width_all(1)
	tsb.set_corner_radius_all(theme.RADIUS_XS)
	type_label.add_theme_stylebox_override("normal", tsb)
	
	# Transition type
	trans_label.text = predicted_transition
	trans_label.add_theme_color_override("font_color", theme.TEXT_SECONDARY)
	trans_label.add_theme_font_override("font", theme.font_ui)
	trans_label.add_theme_font_size_override("font_size", theme.TYPE_2XS)
	
	# Genre
	var genre = meta.get("genre", "Unknown")
	if genre == "Unknown": genre = "--"
	genre_label.text = genre
	genre_label.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
	genre_label.add_theme_font_override("font", theme.font_ui)
	genre_label.add_theme_font_size_override("font_size", theme.TYPE_2XS)
	
	div_label.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
	div_label.add_theme_font_override("font", theme.font_ui)
	div_label.add_theme_font_size_override("font_size", theme.TYPE_2XS)
	
	# Energy
	var energy = int(meta.get("energy_level", 0.5) * 100)
	energy_label.text = "Energy: %d%%" % energy
	energy_label.add_theme_color_override("font_color", theme.TEXT_TERTIARY)
	energy_label.add_theme_font_override("font", theme.font_ui)
	energy_label.add_theme_font_size_override("font_size", theme.TYPE_2XS)
	
	# Card styling
	if is_selected:
		var highlight = StyleBoxFlat.new()
		highlight.bg_color = theme.ACCENT_SURFACE
		highlight.border_color = theme.ACCENT_BRIGHT
		highlight.set_border_width_all(1)
		highlight.set_corner_radius_all(theme.RADIUS_MD)
		add_theme_stylebox_override("normal", highlight)
		add_theme_stylebox_override("hover", highlight)
		add_theme_stylebox_override("pressed", highlight)
	else:
		var sb_normal = StyleBoxFlat.new()
		sb_normal.bg_color = theme.BG_ELEVATED
		sb_normal.border_color = theme.GLASS_BORDER_SUBTLE
		sb_normal.set_border_width_all(1)
		sb_normal.set_corner_radius_all(theme.RADIUS_MD)
		
		var sb_hover = StyleBoxFlat.new()
		sb_hover.bg_color = theme.GLASS_TINT
		sb_hover.border_color = theme.ACCENT_CORE
		sb_hover.set_border_width_all(1)
		sb_hover.set_corner_radius_all(theme.RADIUS_MD)
		
		add_theme_stylebox_override("normal", sb_normal)
		add_theme_stylebox_override("hover", sb_hover)
		add_theme_stylebox_override("pressed", sb_hover)
		add_theme_stylebox_override("focus", ThemeManager.make_transparent())
		
	# Badge overlay additions
	if is_ai_preferred:
		var ai_badge := Label.new()
		ai_badge.text = " 🤖 AI Choice "
		ai_badge.add_theme_color_override("font_color", theme.TEXT_INVERSE)
		ai_badge.add_theme_font_override("font", theme.font_ui)
		ai_badge.add_theme_font_size_override("font_size", theme.TYPE_2XS)
		var aisb = StyleBoxFlat.new()
		aisb.bg_color = theme.ACCENT_CORE
		aisb.set_corner_radius_all(theme.RADIUS_XS)
		ai_badge.add_theme_stylebox_override("normal", aisb)
		ai_badge.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
		ai_badge.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
		overlay_margin.add_child(ai_badge)
		
	if not AudioManager.is_track_cached(href):
		var warn_lbl := Label.new()
		warn_lbl.text = " ⏳ Pending "
		warn_lbl.add_theme_color_override("font_color", theme.TEXT_PRIMARY)
		warn_lbl.add_theme_font_override("font", theme.font_ui)
		warn_lbl.add_theme_font_size_override("font_size", theme.TYPE_2XS)
		var warnsb = StyleBoxFlat.new()
		warnsb.bg_color = theme.SEMANTIC_WARNING
		warnsb.set_corner_radius_all(theme.RADIUS_XS)
		warn_lbl.add_theme_stylebox_override("normal", warnsb)
		warn_lbl.size_flags_horizontal = Control.SIZE_SHRINK_END
		warn_lbl.size_flags_vertical = Control.SIZE_SHRINK_END
		overlay_margin.add_child(warn_lbl)
