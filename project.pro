# 包含主项目的源文件
SOURCES += main.py

# 包含子目录中的所有 Python 文件
SOURCES += src/Core/BeginnerGuidance.py \
           src/Core/CreateScript.py \
           src/Core/GetVersion.py \
           src/Core/NetworkFunc.py \
           src/Core/PathFunc.py \
           src/Core/__init__.py \
           src/Core/Config/ConfigModel.py \
           src/Core/Config/__init__.py \
           src/Ui/Icon.py \
           src/Ui/StyleSheet.py \
           src/Ui/__init__.py \
           src/Ui/AddPage/AddWidget.py \
           src/Ui/AddPage/Advanced.py \
           src/Ui/AddPage/BotWidget.py \
           src/Ui/AddPage/AddTopCard.py \
           src/Ui/AddPage/Connect.py \
           src/Ui/AddPage/__init__.py \
           src/Ui/BotListPage/BotCard.py \
           src/Ui/BotListPage/BotList.py \
           src/Ui/BotListPage/BotListWidget.py \
           src/Ui/BotListPage/BotTopCard.py \
           src/Ui/BotListPage/__init__.py \
           src/Ui/BotListPage/BotWidget/BotInfoPage.py \
           src/Ui/BotListPage/BotWidget/BotSetupPage.py \
           src/Ui/BotListPage/BotWidget/__init__.py \
           src/Ui/common/CodeEditor.py \
           src/Ui/common/__init__.py \
           src/Ui/common/InfoCard/BotListCard.py \
           src/Ui/common/InfoCard/SystemInfoCard.py \
           src/Ui/common/InfoCard/UpdateLogCard.py \
           src/Ui/common/InfoCard/VersionCard.py \
           src/Ui/common/InfoCard/__init__.py \
           src/Ui/common/InputCard/BaseClass.py \
           src/Ui/common/InputCard/GenericCard.py \
           src/Ui/common/InputCard/HttpConfigCard.py \
           src/Ui/common/InputCard/Item.py \
           src/Ui/common/InputCard/UrlCard.py \
           src/Ui/common/InputCard/WsConfigCard.py \
           src/Ui/common/InputCard/__init__.py \
           src/Ui/common/Netwrok/DownloadButton.py \
           src/Ui/common/Netwrok/DownloadCard.py \
           src/Ui/common/Netwrok/UpdateCard.py \
           src/Ui/common/Netwrok/__init__.py \
           src/Ui/HomePage/DisplayView.py \
           src/Ui/HomePage/Home.py \
           src/Ui/HomePage/__init__.py \
           src/Ui/HomePage/ContentView/ContentTopCard.py \
           src/Ui/HomePage/ContentView/DashboardWidget.py \
           src/Ui/HomePage/ContentView/__init__.py \
           src/Ui/HomePage/DownloadView/DownloadTopCard.py \
           src/Ui/HomePage/DownloadView/__init__.py \
           src/Ui/HomePage/UpdateView/TopWidget.py \
           src/Ui/HomePage/UpdateView/__init__.py \
           src/Ui/MainWindow/TitleBar.py \
           src/Ui/MainWindow/Window.py \
           src/Ui/MainWindow/__init__.py \
           src/Ui/SetupPage/Setup.py \
           src/Ui/SetupPage/__init__.py

# 包含的资源文件（如果需要处理 Qt 资源文件）
RESOURCES += src/Ui/resource/resources.qrc

# 包含翻译文件
TRANSLATIONS += src/Ui/resource/i18n/translation.zh_CN.ts \
                src/Ui/resource/i18n/translation.zh_TW.ts
