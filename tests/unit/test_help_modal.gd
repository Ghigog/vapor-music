## test_help_modal.gd
## GUT unit tests for Vibe Workbench help button and help modal.
extends GutTest

var vibe_scene = preload("res://scenes/screens/vibe/vibe_screen.tscn")
var vibe_node: Control

func before_each() -> void:
	vibe_node = vibe_scene.instantiate()
	add_child_autofree(vibe_node)

func test_help_elements_exist() -> void:
	assert_not_null(vibe_node.help_button, "HelpButton must exist")
	assert_not_null(vibe_node.help_modal, "HelpModal must exist")
	assert_not_null(vibe_node.help_modal_panel, "HelpModal panel must exist")
	assert_not_null(vibe_node.help_modal_title, "HelpModal title must exist")
	assert_not_null(vibe_node.help_modal_close, "HelpModal close button must exist")
	assert_not_null(vibe_node.help_modal_text, "HelpModal text label must exist")

func test_help_modal_visibility_toggle() -> void:
	# Hidden by default
	assert_false(vibe_node.help_modal.visible, "HelpModal should be hidden initially")
	
	# Open Help Modal
	vibe_node.help_button.emit_signal("pressed")
	assert_true(vibe_node.help_modal.visible, "HelpModal should be visible after pressing help button")
	
	# Close Help Modal
	vibe_node.help_modal_close.emit_signal("pressed")
	assert_false(vibe_node.help_modal.visible, "HelpModal should be hidden after pressing close button")

func test_markdown_to_bbcode_parser() -> void:
	var markdown = "# Heading 1\n## Heading 2\n### Heading 3\n---\n- List item\n* Bold **text** and `code`."
	var bbcode = vibe_node._parse_markdown_to_bbcode(markdown)
	
	assert_string_contains(bbcode, "[font_size=20][b][color=#9B8EFF]Heading 1[/color][/b][/font_size]", "Should parse h1")
	assert_string_contains(bbcode, "[font_size=16][b][color=#9B8EFF]Heading 2[/color][/b][/font_size]", "Should parse h2")
	assert_string_contains(bbcode, "[font_size=14][b]Heading 3[/b][/font_size]", "Should parse h3")
	assert_string_contains(bbcode, "[center][color=#9494FF]⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯[/color][/center]", "Should parse hr")
	assert_string_contains(bbcode, "• List item", "Should parse bullet list")
	assert_string_contains(bbcode, "Bold [b]text[/b] and [color=#3DD6C8][code]code[/code][/color].", "Should parse inline bold and code")

func test_table_parsing() -> void:
	var markdown_table = "| Head 1 | Head 2 |\n| :--- | :--- |\n| Val 1 | **Val 2** |"
	var bbcode = vibe_node._parse_markdown_to_bbcode(markdown_table)
	
	assert_string_contains(bbcode, "[table=2]", "Should open table")
	assert_string_contains(bbcode, "[cell][b][color=#9B8EFF]Head 1[/color][/b][/cell]", "Should format header cell 1")
	assert_string_contains(bbcode, "[cell][b][color=#9B8EFF]Head 2[/color][/b][/cell]", "Should format header cell 2")
	assert_string_contains(bbcode, "[cell]Val 1[/cell]", "Should format data cell 1")
	assert_string_contains(bbcode, "[cell][b]Val 2[/b][/cell]", "Should format data cell 2 with bold inline formatting")
	assert_string_contains(bbcode, "[/table]", "Should close table")
