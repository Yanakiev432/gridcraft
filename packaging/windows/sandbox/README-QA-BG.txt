GRIDCRAFT V52 — НЕЗАВИСИМ WINDOWS SANDBOX MSI ТЕСТ
==================================================

СЪДЪРЖАНИЕ
- Input/: четири синтетични MSI пакета и автоматичен тест.
- Launch-GridCraft-Sandbox-QA.ps1: създава и стартира изолиран Windows Sandbox.
- INPUT_MANIFEST.json: SHA-256 контролни суми.

ВАЖНО
Не инсталирай MSI файловете ръчно на основния компютър. Това са
НЕПОДПИСАНИ тестови пакети с празни изпълними файлове, а не GridCraft за работа.

ИЗИСКВАНИЯ
Windows 11 Pro/Enterprise, активиран Windows Sandbox и администраторски права
в самата виртуална машина. Скриптът НЕ активира Windows Sandbox и НЕ рестартира PC.
Ако Windows Sandbox не е наличен, тестът не може да се изпълни на тази машина.

СТАРТИРАНЕ
1. Разархивирай целия ZIP в локална папка.
2. Отвори Windows PowerShell в тази папка.
3. Изпълни:
   powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Launch-GridCraft-Sandbox-QA.ps1
4. Изчакай Sandbox да стартира и да приключи инсталационните проверки.
5. Отвори Results\sandbox-acceptance.json.
6. Изпрати файла sandbox-acceptance.json и, при FAIL, Results\console.txt
   и съответния MSI .log файл за диагностика.

Sandbox конфигурацията:
- vGPU е изключен; няма GPU обучение.
- Входните файлове са монтирани само за четене.
- Само Results е достъпна за запис от Sandbox.
- Clipboard, микрофон, камера и принтери са изключени.
- Мрежата е включена само защото Windows Sandbox remote session на Lenovo
  не се свърза при изключена мрежа. Самите QA скриптове не използват интернет.

КРИТЕРИЙ ЗА ПРИЕМАНЕ
В Results\sandbox-acceptance.json трябва да има "pass": true,
"leftover_products": 0 и успешно преминати проверки за:
side-by-side install, repair, upgrade, uninstall, shortcut и default handlers.

Ако Windows Sandbox не се стартира или връзката прекъсне, резултатът НЕ Е PASS.
Не повтаряй тестовете V44/V45/V46/V51 за Excel и B-Trust.

Този тест НЕ заменя публичен Authenticode подпис или юридическия преглед.
