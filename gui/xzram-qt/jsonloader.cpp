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
    if (!value.isDouble()) {
        return defaultValue;
    }
    return static_cast<quint64>(value.toDouble(defaultValue));
}

bool optionalBool(const QJsonObject &obj, const QString &key, bool defaultValue) {
    const QJsonValue value = obj.value(key);
    if (!value.isBool()) {
        return defaultValue;
    }
    return value.toBool(defaultValue);
}

} // namespace JsonLoader
