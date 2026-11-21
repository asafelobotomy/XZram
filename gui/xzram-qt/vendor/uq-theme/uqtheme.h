// uq-theme 1.0.0
//
// uQ's theme tokens ($XDG_STATE_HOME/uq/theme/tokens.json, written by uqd) for a Qt 6 app, and a
// live follow of that file. Vendor uqtheme.h and uqtheme.cpp unchanged (see uQ's
// native/uq-theme/README.md); they need only Qt Core.
//
// load() gives the tokens as one map, the same shape every uQ native app's QML Theme singleton
// reads (and the same as uq_theme.rs):
//
//   {present, name, variant, colors{bg, surface, ...}, radius, gapIn, gapOut, borderWidth,
//    fontUi, fontMono, fontSize, iconTheme}
//
// or {present: false} when uQ isn't there, the file doesn't parse, or a colour is missing or not
// #rrggbb / #rrggbbaa (half a theme looks worse than the desktop's own).
#pragma once

#include <QFileSystemWatcher>
#include <QObject>
#include <QString>
#include <QTimer>
#include <QVariantMap>

namespace uqtheme {

inline constexpr const char *version = "1.0.0";

/// $<overrideVar> when set (tests and screenshots point it at a fixture), else
/// $XDG_STATE_HOME/uq/theme/tokens.json, else ~/.local/state/uq/theme/tokens.json.
QString tokensPath(const char *overrideVar = nullptr);

/// The tokens in `text` (the contents of tokens.json).
QVariantMap parse(const QByteArray &text);

/// The tokens at `path`; {present: false} without them.
QVariantMap load(const QString &path);

/// Follows the tokens file with inotify (QFileSystemWatcher), not a poll. uqd replaces the whole
/// theme/ folder at once, so the folder and its parent are watched, or the nearest folder that
/// exists when uQ hasn't written a theme yet. `tokensChanged` is emitted only on a real change.
class Watcher : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantMap tokens READ tokens NOTIFY tokensChanged)

public:
    explicit Watcher(QString path = tokensPath(), QObject *parent = nullptr);
    QVariantMap tokens() const { return m_tokens; }
    QString path() const { return m_path; }

signals:
    void tokensChanged();

private:
    void reload();
    void rewatch();
    QString m_path;
    QVariantMap m_tokens;
    QFileSystemWatcher m_watcher;
    QTimer m_debounce;
};

} // namespace uqtheme
