// uq-theme 1.1.0 (see uqtheme.h)
#include "uqtheme.h"

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QRegularExpression>

#include <algorithm>

namespace uqtheme {

QString tokensPath(const char *overrideVar)
{
    if (overrideVar) {
        const QString env = qEnvironmentVariable(overrideVar);
        if (!env.isEmpty())
            return env;
    }
    QString state = qEnvironmentVariable("XDG_STATE_HOME");
    if (!QDir::isAbsolutePath(state))
        state = QDir::homePath() + QStringLiteral("/.local/state");
    return state + QStringLiteral("/uq/theme/tokens.json");
}

QVariantMap parse(const QByteArray &text)
{
    const QVariantMap absent{{QStringLiteral("present"), false}};
    QJsonParseError err;
    const QJsonDocument doc = QJsonDocument::fromJson(text, &err);
    if (err.error != QJsonParseError::NoError || !doc.isObject())
        return absent;
    const QJsonObject v = doc.object();
    static const QStringList names = {"bg",     "surface",     "surface_alt", "overlay",
                                      "text",   "text_muted",  "accent",      "accent_text",
                                      "border", "success",     "warning",     "error"};
    static const QRegularExpression colour(QStringLiteral("^#([0-9a-fA-F]{6}|[0-9a-fA-F]{8})$"));
    const QJsonObject in = v.value(QStringLiteral("colors")).toObject();
    QVariantMap colors;
    for (const QString &k : names) {
        const QString c = in.value(k).toString();
        if (!colour.match(c).hasMatch())
            return absent;
        colors.insert(k, c);
    }
    const QJsonObject ui = v.value(QStringLiteral("ui")).toObject();
    const QJsonObject fonts = v.value(QStringLiteral("fonts")).toObject();
    const auto num = [](const QJsonObject &o, const char *key, int def, int max) {
        const QJsonValue x = o.value(QLatin1String(key));
        return x.isDouble() && x.toDouble() >= 0 ? std::min(x.toInt(def), max) : def;
    };
    const QJsonObject motion = v.value(QStringLiteral("motion")).toObject();
    const bool reduce = motion.value(QStringLiteral("reduce")).toBool(false);
    const auto ms = [&](const char *key, int def) { return reduce ? 0 : num(motion, key, def, 1000); };
    const bool light = v.value(QStringLiteral("variant")).toString() == QLatin1String("light");
    return {
        {QStringLiteral("present"), true},
        {QStringLiteral("name"), v.value(QStringLiteral("name")).toString(QStringLiteral("uQ"))},
        {QStringLiteral("variant"), light ? QStringLiteral("light") : QStringLiteral("dark")},
        {QStringLiteral("colors"), colors},
        {QStringLiteral("radius"), num(ui, "radius", 8, 32)},
        {QStringLiteral("gapIn"), num(ui, "gap_in", 5, 64)},
        {QStringLiteral("gapOut"), num(ui, "gap_out", 12, 64)},
        {QStringLiteral("borderWidth"), num(ui, "border_width", 1, 8)},
        {QStringLiteral("fontUi"), fonts.value(QStringLiteral("ui")).toString()},
        {QStringLiteral("fontMono"), fonts.value(QStringLiteral("mono")).toString()},
        {QStringLiteral("fontSize"), num(fonts, "size", 0, 32)},
        {QStringLiteral("iconTheme"),
         v.value(QStringLiteral("icons")).toObject().value(QStringLiteral("theme")).toString()},
        {QStringLiteral("motion"),
         QVariantMap{{QStringLiteral("fast"), ms("fast", 100)},
                     {QStringLiteral("base"), ms("base", 140)},
                     {QStringLiteral("slow"), ms("slow", 220)},
                     {QStringLiteral("reduce"), reduce}}},
    };
}

QVariantMap load(const QString &path)
{
    QFile f(path);
    if (path.isEmpty() || !f.open(QIODevice::ReadOnly) || f.size() > 256 * 1024)
        return {{QStringLiteral("present"), false}};
    return parse(f.readAll());
}

Watcher::Watcher(QString path, QObject *parent)
    : QObject(parent), m_path(std::move(path)), m_tokens(load(m_path))
{
    // uqd writes several files in a burst; let it finish, then read once
    m_debounce.setSingleShot(true);
    m_debounce.setInterval(150);
    connect(&m_debounce, &QTimer::timeout, this, &Watcher::reload);
    connect(&m_watcher, &QFileSystemWatcher::fileChanged, &m_debounce, qOverload<>(&QTimer::start));
    connect(&m_watcher, &QFileSystemWatcher::directoryChanged, &m_debounce,
            qOverload<>(&QTimer::start));
    rewatch();
}

void Watcher::rewatch()
{
    const QStringList have = m_watcher.files() + m_watcher.directories();
    if (!have.isEmpty())
        m_watcher.removePaths(have);
    if (m_path.isEmpty())
        return;
    QString dir = QFileInfo(m_path).absolutePath();
    while (!QFileInfo(dir).isDir() && dir != QLatin1String("/"))
        dir = QFileInfo(dir).absolutePath();
    m_watcher.addPath(dir);
    const QString up = QFileInfo(dir).absolutePath();
    if (up != dir)
        m_watcher.addPath(up);
    if (QFileInfo::exists(m_path))
        m_watcher.addPath(m_path);
}

void Watcher::reload()
{
    rewatch();
    const QVariantMap next = load(m_path);
    if (next != m_tokens) {
        m_tokens = next;
        emit tokensChanged();
    }
}

} // namespace uqtheme
