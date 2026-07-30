#include "register_types.h"

#include "audio_dsp.h"

#include <gdextension_interface.h>
#include <godot_cpp/core/defs.hpp>
#include <godot_cpp/godot.hpp>
#include <essentia/essentia.h>

using namespace godot;

void audio_dsp_library_init(ModuleInitializationLevel p_level) {
	if (p_level != MODULE_INITIALIZATION_LEVEL_SCENE) {
		return;
	}

	essentia::init();
	ClassDB::register_class<AudioDSP>();
#ifdef VAPOR_HAS_MEDIA_CONTROLS
	ClassDB::register_class<MediaControls>();
#endif
}

void audio_dsp_library_terminate(ModuleInitializationLevel p_level) {
	if (p_level != MODULE_INITIALIZATION_LEVEL_SCENE) {
		return;
	}

	essentia::shutdown();
}

extern "C" {
// Initialization.
GDExtensionBool GDE_EXPORT audio_dsp_library_init(GDExtensionInterfaceGetProcAddress p_get_proc_address, const GDExtensionClassLibraryPtr p_library, GDExtensionInitialization *r_initialization) {
	godot::GDExtensionBinding::InitObject init_obj(p_get_proc_address, p_library, r_initialization);

	init_obj.register_initializer(audio_dsp_library_init);
	init_obj.register_terminator(audio_dsp_library_terminate);
	init_obj.set_minimum_library_initialization_level(MODULE_INITIALIZATION_LEVEL_SCENE);

	return init_obj.init();
}
}
