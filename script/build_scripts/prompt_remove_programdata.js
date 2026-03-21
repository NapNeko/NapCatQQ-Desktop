var customActionData = Session.Property("CustomActionData");

if (customActionData) {
    var parts = customActionData.split("|");
    var dataPath = parts.length > 0 ? parts[0] : "";
    var uiLevel = parts.length > 1 ? parseInt(parts[1], 10) : 5;

    if (!isNaN(uiLevel) && uiLevel >= 5 && dataPath) {
        var fileSystem = new ActiveXObject("Scripting.FileSystemObject");

        if (fileSystem.FolderExists(dataPath)) {
            var shell = new ActiveXObject("WScript.Shell");
            var message =
                "Delete NapCatQQ Desktop data stored in ProgramData?\n\n" +
                dataPath +
                "\n\nClick Yes to remove runtime, config, temp, and log data.";
            var response = shell.Popup(message, 0, "NapCatQQ Desktop", 4 + 32 + 256);

            if (response === 6) {
                var folder = fileSystem.GetFolder(dataPath);
                clearReadOnlyAttributes(folder);
                folder.Attributes = folder.Attributes & ~1;
                fileSystem.DeleteFolder(dataPath, true);
            }
        }
    }
}

function clearReadOnlyAttributes(folder) {
    var fileItems = new Enumerator(folder.Files);
    for (; !fileItems.atEnd(); fileItems.moveNext()) {
        var fileItem = fileItems.item();
        fileItem.Attributes = fileItem.Attributes & ~1;
    }

    var subFolders = new Enumerator(folder.SubFolders);
    for (; !subFolders.atEnd(); subFolders.moveNext()) {
        var subFolder = subFolders.item();
        clearReadOnlyAttributes(subFolder);
        subFolder.Attributes = subFolder.Attributes & ~1;
    }
}
