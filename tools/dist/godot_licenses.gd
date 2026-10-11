extends SceneTree
# Godot's licence and the notices of the code it includes, from the Godot
# that exports (its templates are the same version), for legal/ in a package:
#   godot --headless --path <any project> -s godot_licenses.gd -- <out dir>


func _init() -> void:
	var out: String = OS.get_cmdline_user_args()[0]
	_write(out.path_join("Godot-LICENSE.txt"), Engine.get_license_text())
	var text := "Godot Engine %s includes the following third-party code.\n" % [
		Engine.get_version_info()["string"]]
	for info in Engine.get_copyright_info():
		text += "\n%s\n" % info["name"]
		for part in info["parts"]:
			for line in part["copyright"]:
				text += "  Copyright %s\n" % line
			text += "  License: %s\n" % part["license"]
	text += "\n\nLicense texts\n"
	var licenses: Dictionary = Engine.get_license_info()
	for name in licenses:
		text += "\n----- %s -----\n\n%s\n" % [name, licenses[name]]
	_write(out.path_join("Godot-COPYRIGHT.txt"), text)
	quit()


func _write(path: String, text: String) -> void:
	var f := FileAccess.open(path, FileAccess.WRITE)
	f.store_string(text)
	f.close()
