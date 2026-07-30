#ifndef REGISTRATION_TYPES_H
#define REGISTRATION_TYPES_H

#include <godot_cpp/core/class_db.hpp>

#if defined(__APPLE__)
#include "platform/macos/media_controls_macos.h"
#define VAPOR_HAS_MEDIA_CONTROLS 1
#elif defined(_WIN32)
#include "platform/windows/media_controls_windows.h"
#define VAPOR_HAS_MEDIA_CONTROLS 1
#endif

using namespace godot;

void audio_dsp_library_init(ModuleInitializationLevel p_level);
void audio_dsp_library_terminate(ModuleInitializationLevel p_level);

#endif // REGISTRATION_TYPES_H
