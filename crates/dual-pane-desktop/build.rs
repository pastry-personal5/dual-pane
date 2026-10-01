use cxx_qt_build::CxxQtBuilder;

fn main() {
    // SAFETY: The callback only adds this crate's checked-in private header directory. It does
    // not change CXX-Qt's generated sources, linkage, or compiler configuration.
    unsafe {
        CxxQtBuilder::new()
            .file("src/folder_items_list_model.rs")
            .cpp_file("cpp/src/desktop_window.cpp")
            .qt_module("Widgets")
            .cc_builder(|builder| {
                builder.include("cpp/include");
            })
            .build();
    }
}
