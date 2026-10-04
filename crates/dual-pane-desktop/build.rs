use cxx_qt_build::CxxQtBuilder;

fn main() {
    // SAFETY: The callback only adds this crate's checked-in private header directory. It does
    // not change CXX-Qt's generated sources, linkage, or compiler configuration.
    unsafe {
        CxxQtBuilder::new()
            .file("src/folder_items_list_model.rs")
            .file("src/workspace_bridge.rs")
            .file("src/native_shell.rs")
            .file("src/operations_bridge.rs")
            .cpp_file("cpp/src/desktop_window.cpp")
            .cpp_file("cpp/src/native_shell.cpp")
            .cpp_file("cpp/src/quick_look_preview.mm")
            .qt_module("Widgets")
            .cc_builder(|builder| {
                builder.include("cpp/include");
                builder.flag_if_supported("-fobjc-arc");
            })
            .build();
    }
    println!("cargo:rustc-link-lib=framework=QuickLookUI");
}
