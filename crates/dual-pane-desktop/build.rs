use cxx_qt_build::CxxQtBuilder;

fn main() {
    // SAFETY: The callback only adds this crate's checked-in private header directory. It does
    // not change CXX-Qt's generated sources, linkage, or compiler configuration.
    unsafe {
        CxxQtBuilder::new()
            .file("src/folder_items_list_model.rs")
            .file("src/workspace_bridge.rs")
            .file("src/location_watcher.rs")
            .file("src/native_shell.rs")
            .file("src/operations_bridge.rs")
            .cpp_file("cpp/src/desktop_window.cpp")
            .cpp_file("cpp/src/gui_common.cpp")
            .cpp_file("cpp/src/folder_pane.cpp")
            .cpp_file("cpp/src/workspace_window.cpp")
            .cpp_file("cpp/src/shortcuts.cpp")
            .cpp_file("cpp/src/toolbar_sidebar.cpp")
            .cpp_file("cpp/src/desktop_checks.cpp")
            .cpp_file("cpp/src/preview_reader.cpp")
            .cpp_file("cpp/src/native_shell.cpp")
            .cpp_file("cpp/src/file_associations.mm")
            .cpp_file("cpp/src/quick_look_preview.mm")
            .cpp_file("cpp/src/keyboard_layout.mm")
            .cpp_file("cpp/src/clipboard_adapter.mm")
            .cpp_file("cpp/src/application_activation.mm")
            .cpp_file("cpp/src/location_watcher.cpp")
            .qt_module("Widgets")
            .cc_builder(|builder| {
                builder.include("cpp/include");
                builder.flag_if_supported("-fobjc-arc");
            })
            .build();
    }
    println!("cargo:rustc-link-lib=framework=QuickLookUI");
    println!("cargo:rustc-link-lib=framework=CoreServices");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=UniformTypeIdentifiers");
}
