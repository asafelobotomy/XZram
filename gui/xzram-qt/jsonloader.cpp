#include "jsonloader.h"

#include <QJsonDocument>
#include <QJsonValue>

namespace JsonLoader {

QJsonObject parseObject(const QString &json, QString *error) {
    QJsonParseError parseError;
    const QJsonDocument doc = QJsonDocument::fromJson(json.toUtf8(), &parseError);
    if (parseError.error != QJsonParseError::NoError || !doc.isObject()) {
        if (error) {
            *error = parseError.errorString();
        }
        return {};
    }
    return doc.object();
}

QJsonArray parseArray(const QString &json, QString *error) {
    QJsonParseError parseError;
    const QJsonDocument doc = QJsonDocument::fromJson(json.toUtf8(), &parseError);
    if (parseError.error != QJsonParseError::NoError || !doc.isArray()) {
        if (error) {
            *error = parseError.errorString();
        }
        return {};
    }
    return doc.array();
}

QString optionalString(const QJsonObject &obj, const QString &key) {
    const QJsonValue value = obj.value(key);
    if (value.isNull() || value.isUndefined()) {
        return {};
    }
    if (value.isString()) {
        return value.toString();
    }
    if (value.isDouble()) {
        return QString::number(value.toDouble());
    }
    if (value.isBool()) {
        return value.toBool() ? QStringLiteral("true") : QStringLiteral("false");
    }
    return value.toVariant().toString();
}

int optionalInt(const QJsonObject &obj, const QString &key, int defaultValue) {
    const QJsonValue value = obj.value(key);
    if (!value.isDouble()) {
        return defaultValue;
    }
    return value.toInt(defaultValue);
}

quint64 optionalUInt64(const QJsonObject &obj, const QString &key, quint64 defaultValue) {
    const QJsonValue value = obj.value(key);
    if (value.isNull() || value.isUndefined()) {
        return defaultValue;
    }
    if (value.isString()) {
        bool ok = false;
        const quint64 parsed = value.toString().toULongLong(&ok);
        return ok ? parsed : defaultValue;
    }
    if (value.isDouble() || value.isBool()) {
        // Prefer QVariant path over double cast to reduce precision loss near 2^53.
        bool ok = false;
        const quint64 parsed = value.toVariant().toULongLong(&ok);
        return ok ? parsed : defaultValue;
    }
    return defaultValue;
}

bool optionalBool(const QJsonObject &obj, const QString &key, bool defaultValue) {
    const QJsonValue value = obj.value(key);
    if (!value.isBool()) {
        return defaultValue;
    }
    return value.toBool(defaultValue);
}

} // namespace JsonLoader
