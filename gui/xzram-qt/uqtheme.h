#pragma once

#include <QColor>
#include <QFont>
#include <QHash>
#include <QObject>
#include <QString>
#include <QVariantMap>

class QApplication;
class QWidget;
namespace uqtheme {
class Watcher;
}

// Follows the uQ desktop theme. uqd renders ~/.local/state/uq/theme/tokens.json; uQ's shared
// uq-theme module (vendor/uq-theme, copied unchanged from uQ's native/uq-theme) reads and watches
// it, and this applies a palette (Fusion style, so every control follows the palette) plus the UI
// font, and re-applies whenever the tokens change, so a running window re-themes live.
//
// Without a tokens file (not running under uQ) or with XZRAM_THEME=system the system palette
// is kept and the semantic colours are derived from it.
//
// Widgets never carry colours of their own: they set a role (setRole) and the application
// stylesheet, rebuilt on every theme change, styles it.
class UqTheme : public QObject {
    Q_OBJECT
public:
    // Creates the theme for `app`, applies it, and starts watching for changes.
    static UqTheme *install(QApplication *app);
    static UqTheme *instance();

    // Roles: "strip" (neutral info), "ok", "warn", "error", "info", "muted" (plain dim text).
    static void setRole(QWidget *widget, const char *role);

    // A semantic colour ("success", "warning", "error", "accent", "text_muted", ...) for
    // painting that cannot go through the stylesheet.
    QColor color(const QString &token) const;

    // Pure helpers, public for tests.
    static QString tokensPath();
    static QString buildStyleSheet(const QColor &bg, const QColor &surface, const QColor &text,
                                   const QColor &muted, const QColor &accent, const QColor &ok,
                                   const QColor &warn, const QColor &err);
    static QColor mix(const QColor &a, const QColor &b, qreal t);

signals:
    void changed();

private:
    explicit UqTheme(QApplication *app);
    void reload();
    bool applyTokens();
    void applySystem();

    QApplication *m_app;
    uqtheme::Watcher *m_tokens;
    QVariantMap m_applied;  // tokens last applied, to skip no-op reloads
    QHash<QString, QColor> m_colors;
    QString m_defaultStyle;
    QFont m_defaultFont;
    bool m_system = false;
};
