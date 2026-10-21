#include "dual_pane_desktop/folder_pane.hpp"

// Qt parent ownership and the compact native composition are explicit here.
// NOLINTBEGIN(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
namespace dual_pane_desktop::gui {

auto filter_status_text(int status, bool no_matches, const QString &text, int count) -> QString {
    if (status == 1)
        return QStringLiteral("Updating…");
    if (status == 2)
        return QStringLiteral("Filter unavailable; edit to retry");
    if (no_matches)
        return QStringLiteral("No matches");
    if (text.isEmpty())
        return QStringLiteral("%1 items").arg(count);
    return QStringLiteral("%1 matches").arg(count);
}

} // namespace dual_pane_desktop::gui
// NOLINTEND(cppcoreguidelines-owning-memory,cppcoreguidelines-avoid-magic-numbers,readability-magic-numbers,readability-braces-around-statements,readability-identifier-length,readability-isolate-declaration,readability-named-parameter,bugprone-easily-swappable-parameters,modernize-use-trailing-return-type,readability-function-cognitive-complexity,readability-convert-member-functions-to-static,cppcoreguidelines-special-member-functions,readability-implicit-bool-conversion)
