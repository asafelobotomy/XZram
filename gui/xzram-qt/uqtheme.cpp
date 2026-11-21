#include "uqtheme.h"
#include "vendor/uq-theme/uqtheme.h"

#include <QApplication>
#include <QFontDatabase>
#include <QPalette>
#include <QStyle>
#include <QStyleFactory>
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

QString UqTheme::tokensPath() { return uqtheme::tokensPath("XZRAM_UQ_TOKENS"); }

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
    // uq-theme follows the file (and uqd's swap of the whole theme folder) with a file watch
    m_tokens = new uqtheme::Watcher(tokensPath(), this);
    connect(m_tokens, &uqtheme::Watcher::tokensChanged, this, &UqTheme::reload);
    reload();
}

bool UqTheme::applyTokens() {
    const QVariantMap t = m_tokens->tokens();
    if (!t.value(QStringLiteral("present")).toBool()) {
        return false;  // no uQ, or not a whole theme: keep what is applied
    }
    if (t == m_applied && !m_system) {
        return true;
    }
    m_applied = t;
    m_system = false;
    const QVariantMap colors = t.value(QStringLiteral("colors")).toMap();
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

    const QString family = t.value(QStringLiteral("fontUi")).toString();
    QFont font = m_defaultFont;
    if (!family.isEmpty() && QFontDatabase::families().contains(family)) {
        font.setFamily(family);
    }
    if (const int size = t.value(QStringLiteral("fontSize")).toInt(); size > 0) {
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
    const bool ok = !optOut && applyTokens();
    if (!ok && !hadTokens) {
        applySystem();
    }
    emit changed();
}
