#include "mainwindow.h"

#include <QApplication>

int main(int argc, char *argv[]) {
    QApplication app(argc, argv);
  QApplication::setApplicationName(QStringLiteral("xzram-qt"));
  QApplication::setOrganizationName(QStringLiteral("XZram"));

    MainWindow window;
    window.show();
    return app.exec();
}
