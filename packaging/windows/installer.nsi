!define APPNAME "Hyper Lang (hyperc)"
!define COMPANY "Hyper Lang Contributors"
!define VERSION "${VERSION}"
!define EXE_NAME "hyperc.exe"

Name "${APPNAME}"
OutFile "hyperc-${VERSION}-windows-setup.exe"
InstallDir "$PROGRAMFILES64\HyperLang"
RequestExecutionLevel admin

Page directory
Page instfiles

UninstPage uninstConfirm
UninstPage instfiles

Section "Install"
  SetOutPath "$INSTDIR"
  File "dist\hyperc-windows-amd64\hyperc.exe"
  File "LICENSE"
  File "README.adoc"

  ; dopisanie do PATH użytkownika, żeby `hyperc` działało z dowolnego katalogu
  EnVar::SetHKLM
  EnVar::AddValue "PATH" "$INSTDIR"

  WriteUninstaller "$INSTDIR\uninstall.exe"

  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\HyperLang" \
    "DisplayName" "${APPNAME}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\HyperLang" \
    "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\HyperLang" \
    "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\HyperLang" \
    "Publisher" "${COMPANY}"
SectionEnd

Section "Uninstall"
  EnVar::SetHKLM
  EnVar::DeleteValue "PATH" "$INSTDIR"

  Delete "$INSTDIR\${EXE_NAME}"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\README.adoc"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\HyperLang"
SectionEnd
