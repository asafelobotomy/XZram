#include "uqtheme.h"

#include <QApplication>
#include <QDir>
#include <QFile>
#include <QFileSystemWatcher>
#include <QFontDatabase>
#include <QJsonDocument>
#include <QJsonObject>
#include <QPalette>
#include <QStandardPaths>
#include <QStyle>
#include <QStyleFactory>
#include <QTimer>
#include <QWidget>

namespace {
UqTheme *g_instance = nullptr;

QString css(const QColor &c) { return c.name(QColor::HexRgb); }

QString roleRule(const char *role, const QColor &bg, const QColor &fg, const QColor &border,
                 const QColor &tint, qreal amount) {
    const QColor fill = UqTheme::mix(bg, tint, amount);
    return QStringLiteral(
               "[uq=\"%1\"] { background: %2; color: %3; border: 1px solid %4; "
               "border-radius: 4px; padding: 6px; }")
        .arg(QLatin1String(role), css(fill), css(fg), css(UqTheme::mix(bg, border, 0.55)));
}
} // namespace

UqTheme *UqTheme::instance() { return g_instance; }

UqTheme *UqTheme::install(QApplication *app) {
    if (!g_instance) {
        g_instance = new UqTheme(app);
    }
    return g_instance;
}

QString UqTheme::tokensPath() {
    QString state = qEnvironmentVariable("XDG_STATE_HOME");
    if (state.isEmpty()) {
        state = QDir::homePath() + QStringLiteral("/.local/state");
    }
    return state + QStringLiteral("/uq/theme/tokens.json");
}

QColor UqTheme::mix(const QColor &a, const QColor &b, qreal t) {
    return QColor::fromRgbF(a.redF() + (b.redF() - a.redF()) * t,
                            a.greenF() + (b.greenF() - a.greenF()) * t,
                            a.blueF() + (b.blueF() - a.blueF()) * t);
}

QString UqTheme::buildStyleSheet(const QColor &bg, const QColor &surface, const QColor &text,
                                 const QColor &muted, const QColor &accent, const QColor &ok,
                                 const QColor &warn, const QColor &err) {
    QStringList rules;
    rules << roleRule("strip", bg, muted, muted, surface, 1.0)
          << roleRule("ok", bg, text, ok, ok, 0.2)
          << roleRule("warn", bg, text, warn, warn, 0.2)
          << roleRule("error", bg, text, err, err, 0.2)
          << roleRule("info", bg, text, accent, accent, 0.2)
          << QStringLiteral("[uq=\"muted\"] { color: %1; }").arg(css(muted))
          << QStringLiteral("PendingBanner { background: %1; border: 1px solid %2; "
                            "border-radius: 4px; }")
                 .arg(css(mix(bg, warn, 0.2)), css(mix(bg, warn, 0.55)));
    return rules.join(QLatin1Char('\n'));
}

void UqTheme::setRole(QWidget *widget, const char *role) {
    widget->setProperty("uq", QString::fromLatin1(role));
    // Re-evaluate the selector when a widget switches role at runtime.
    widget->style()->unpolish(widget);
    widget->style()->polish(widget);
    widget->update();
}

QColor UqTheme::color(const QString &token) const {
    return m_colors.value(token, QColor(Qt::gray));
}

UqTheme::UqTheme(QApplication *app) : QObject(app), m_app(app) {
    m_defaultStyle = app->style()->objectName();
    m_defaultFont = app->font();
    m_watcher = new QFileSystemWatcher(this);
    m_debounce = new QTimer(this);
    m_debounce->setSingleShot(true);
    m_debounce->setInterval(120);
    connect(m_debounce, &QTimer::timeout, this, &UqTheme::reload);
    // uqd rewrites by rename, which drops a file watch: watch the directory as well, and
    // re-arm both after every event.
    connect(m_watcher, &QFileSystemWatcher::fileChanged, m_debounce, qOverload<>(&QTimer::start));
    connect(m_watcher, &QFileSystemWatcher::directoryChanged, m_debounce,
            qOverload<>(&QTimer::start));
    reload();
}

void UqTheme::rewatch() {
    const QString path = tokensPath();
    const QString dir = QFileInfo(path).absolutePath();
    // Watch the nearest existing ancestor so a theme dir created later is noticed too.
    QString watchDir = dir;
    while (!QDir(watchDir).exists() && watchDir.length() > 1) {
        watchDir = QFileInfo(watchDir).absolutePath();
    }
    const QStringList have = m_watcher->files() + m_watcher->directories();
    if (!have.isEmpty()) {
        m_watcher->removePaths(have);
    }
    m_watcher->addPath(watchDir);
    if (QFile::exists(path)) {
        m_watcher->addPath(path);
    }
}

bool UqTheme::loadTokens() {
    QFile file(tokensPath());
    if (!file.open(QIODevice::ReadOnly)) {
        return false;
    }
    const QByteArray raw = file.readAll();
    const QJsonObject root = QJsonDocument::fromJson(raw).object();
    const QJsonObject colors = root.value(QStringLiteral("colors")).toObject();
    static const QStringList required = {
        QStringLiteral("bg"),   QStringLiteral("surface"), QStringLiteral("text"),
        QStringLiteral("accent"), QStringLiteral("error")};
    for (const QString &key : required) {
        if (!QColor(colors.value(key).toString()).isValid()) {
            return false;  // half-written or foreign file: keep what is applied
        }
    }
    if (QString::fromUtf8(raw) == m_applied && !m_system) {
        return true;
    }
    m_applied = QString::fromUtf8(raw);
    m_system = false;
    m_colors.clear();
    for (auto it = colors.begin(); it != colors.end(); ++it) {
        const QColor c(it.value().toString());
        if (c.isValid()) {
            m_colors.insert(it.key(), c);
        }
    }
    auto col = [&](const char *key, const char *fallback) {
        return m_colors.value(QLatin1String(key), m_colors.value(QLatin1String(fallback)));
    };
    const QColor bg = col("bg", "bg"), surface = col("surface", "bg"),
                 alt = col("surface_alt", "surface"), overlay = col("overlay", "surface_alt"),
                 text = col("text", "text"), muted = col("text_muted", "text"),
                 accent = col("accent", "accent"), accentText = col("accent_text", "bg"),
                 border = col("border", "overlay"), ok = col("success", "accent"),
                 warn = col("warning", "accent"), err = col("error", "error");

    QPalette p;
    p.setColor(QPalette::Window, bg);
    p.setColor(QPalette::WindowText, text);
    p.setColor(QPalette::Base, surface);
    p.setColor(QPalette::AlternateBase, alt);
    p.setColor(QPalette::Text, text);
    p.setColor(QPalette::Button, alt);
    p.setColor(QPalette::ButtonText, text);
    p.setColor(QPalette::BrightText, err);
    p.setColor(QPalette::ToolTipBase, overlay);
    p.setColor(QPalette::ToolTipText, text);
    p.setColor(QPalette::PlaceholderText, muted);
    p.setColor(QPalette::Highlight, accent);
    p.setColor(QPalette::HighlightedText, accentText);
    p.setColor(QPalette::Link, accent);
    p.setColor(QPalette::LinkVisited, accent);
    p.setColor(QPalette::Light, mix(alt, text, 0.25));
    p.setColor(QPalette::Midlight, overlay);
    p.setColor(QPalette::Mid, border);
    p.setColor(QPalette::Dark, mix(bg, Qt::black, 0.35));
    p.setColor(QPalette::Shadow, Qt::black);
    for (auto role : {QPalette::WindowText, QPalette::Text, QPalette::ButtonText}) {
        p.setColor(QPalette::Disabled, role, muted);
    }

    // Fusion is the style that follows a palette completely; the platform default may not.
    if (QStyleFactory::keys().contains(QStringLiteral("Fusion"), Qt::CaseInsensitive)) {
        m_app->setStyle(QStyleFactory::create(QStringLiteral("Fusion")));
    }
    m_app->setPalette(p);

    const QJsonObject fonts = root.value(QStringLiteral("fonts")).toObject();
    const QString family = fonts.value(QStringLiteral("ui")).toString();
    QFont font = m_defaultFont;
    if (!family.isEmpty() && QFontDatabase::families().contains(family)) {
        font.setFamily(family);
    }
    if (const int size = fonts.value(QStringLiteral("size")).toInt(); size > 0) {
        font.setPointSize(size);
    }
    m_app->setFont(font);

    m_app->setStyleSheet(buildStyleSheet(bg, surface, text, muted, accent, ok, warn, err));
    return true;
}

void UqTheme::applySystem() {
    m_applied.clear();
    m_system = true;
    // Semantic colours derived from the system palette, light or dark.
    const QPalette p = QApplication::palette();
    const QColor bg = p.color(QPalette::Window), text = p.color(QPalette::WindowText);
    const QColor muted = mix(text, bg, 0.4);
    m_colors = {{QStringLiteral("success"), QColor(0x2e, 0x9e, 0x4f)},
                {QStringLiteral("warning"), QColor(0xd9, 0x9a, 0x06)},
                {QStringLiteral("error"), QColor(0xd0, 0x34, 0x3b)},
                {QStringLiteral("accent"), p.color(QPalette::Highlight)},
                {QStringLiteral("text_muted"), muted}};
    m_app->setStyleSheet(buildStyleSheet(bg, p.color(QPalette::AlternateBase), text, muted,
                                         m_colors[QStringLiteral("accent")],
                                         m_colors[QStringLiteral("success")],
                                         m_colors[QStringLiteral("warning")],
                                         m_colors[QStringLiteral("error")]));
}

void UqTheme::reload() {
    const bool optOut = qEnvironmentVariable("XZRAM_THEME") == QLatin1String("system");
    const bool hadTokens = !m_applied.isEmpty();
    const bool ok = !optOut && loadTokens();
    if (!ok && !hadTokens) {
        applySystem();
    }
    rewatch();
    emit changed();
}
