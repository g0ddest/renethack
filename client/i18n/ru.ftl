# renethack: собственный интерфейс клиента, по-русски (ключи те же, что
# в en.ftl). Слова движка (сообщения, названия, меню) здесь не
# переводятся: их переводит nh-i18n.

## Заставка, настройки, создание персонажа, конец игры, ошибки

title-subtitle = NetHack 5.0
title-new-game = Новая игра
title-continue = Продолжить: { $name } ({ $time })
title-settings = Настройки
title-quit = Выход

settings-title = Настройки
settings-language = Язык
settings-back = Назад

creation-title = Новый персонаж
creation-name = Имя
creation-role = Роль
creation-race = Раса
creation-gender = Пол
creation-alignment = Мировоззрение
creation-keys = Клавиши
creation-keys-modern = Современные: стрелки и цифровой блок, n — счётчик
creation-keys-classic = Классические: hjklyubn, Alt+цифры — счётчик
creation-keys-tip = В обоих панель действий — на клавишах 1–0. Современные играют с number_pad (k — пинок, j — прыжок, l — добыча); классические — с vi-клавишами NetHack.
creation-language = Язык
creation-random = Случайно
creation-start = Начать
creation-back = Назад
creation-continue-instead = Продолжить { $name }
creation-name-taken = Есть сохранённая игра с этим именем: начав, вы продолжите её.

end-title = Игра окончена
end-new-game = Новая игра
end-to-title = В меню

error-title = Что-то пошло не так
error-continue = Продолжить { $name }
error-to-title = В меню
error-quit = Выход

## Строка подсказки, приказы и почему они прерываются

prompt-press-key = Нажмите любую клавишу
prompt-more = --Дальше--  (любая клавиша)
prompt-saving = Сохранение игры...
prompt-pick-spot = Выберите место.
prompt-getpos = { $goal }  (. , ; : выбрать, ? справка, Esc: отмена)
order-walk = Иду
order-walk-pick-up = Иду подобрать
order-stairs-up = Иду к лестнице вверх
order-stairs-down = Иду к лестнице вниз
order-open-door = Иду открыть дверь
order-attack = Иду в атаку
order-search = Поиск, ещё { $left }
order-wait = Ожидание, ещё { $left }
order-walk-steps =
    Иду, ещё { $left } { $left ->
        [one] шаг
        [few] шага
       *[many] шагов
    }
order-hold = Клавиша зажата
order-rest = Отдых, пока здоровье и энергия не восстановятся
order-line = { $what }  —  любая клавиша остановит
stop-arrived = На месте
stop-done = Готово
stop-healed = Отдых окончен: здоровье и энергия полны
stop-one-action = В бою — одно действие за раз
stop-released = Клавиша отпущена
stop-stopped = Остановлено
stop-panel = Остановлено: открыта панель
stop-focus = Остановлено: окно потеряло фокус
stop-question = Остановлено: вопрос
stop-hostile = Остановлено: враг в поле зрения
stop-hurt = Остановлено: вы ранены
stop-hunger = Остановлено: изменился голод
stop-condition = Остановлено: изменилось ваше состояние
stop-message = Остановлено: { $message }
stop-level = Остановлено: другой уровень
stop-blocked = Остановлено: путь прегражден
stop-no-path = Пути туда не известно
click-unexplored = не исследовано
click-off-map = вне карты
click-nothing = Там нечего делать ({ $why })

## Панель действий

bar-bound = Назначено на ячейку { $key }
bar-cleared = Ячейка { $key } очищена
bar-restored = Ячейка { $key } восстановлена
bar-slot-empty-tip = Ячейка { $key } (пусто): перетащите сюда предмет из инвентаря
bar-slot-gone-tip = { $what }: { $item } (нет в рюкзаке)
bar-slot-tip = { $what }: { $item }
bar-slot-keys-tip =
    { $tip }
    Клавиша { $key } · NetHack: { $hint } · правый щелчок: очистить

## Ошибки и игровая папка

title-game-saved = Игра сохранена: { $name }.
err-engine-not-built = Игровой движок не собран.
err-engine-not-built-details =
    { $error }
    Выполните `make` в репозитории или задайте RENETHACK_ENGINE_DIR.
err-engine-hangs = Движок перестал отвечать ({ $secs } с без вывода).
err-engine-failed = Сбой игрового движка: { $what }
err-game-saved-as = Игра сохранена; её можно продолжить как { $name }.
err-killed-by-signal = завершён сигналом
err-exit-code = код выхода { $code }
err-engine-stopped = Игровой движок неожиданно остановился ({ $code }).
err-already-running = renethack уже запущен с этой игровой папкой.
err-already-running-details =
    { $dir }
    Сначала закройте другое окно, затем нажмите «В меню».
err-playground = Не удаётся подготовить игровую папку.
err-engine-does-not-start = Игровой движок не запускается.
err-no-catalog = Каталог игры недоступен.
err-cannot-restore = Это сохранение нельзя восстановить.
err-cannot-start = Не удаётся запустить игровой движок.
recover-cannot-look = Не удаётся найти прерванные игры: { $error }
recover-saved = Восстановлена прерванная игра: { $name }.
recover-lost = Прерванную игру ({ $base }) восстановить не удалось.
recover-failed = Восстановить { $base } не удалось: { $error }

## Интерфейс поверх игры

deadly-stone = Вы каменеете!
deadly-slime = Вы превращаетесь в слизь!
deadly-strangled = Удушье!
deadly-food-poisoning = Пищевое отравление!
deadly-terminally-ill = Смертельная болезнь!
attr-st = Сил
attr-dx = Лов
attr-co = Тел
attr-in = Инт
attr-wi = Мдр
attr-ch = Хар
hud-attrs-tip = Характеристики. Щелчок: ваш персонаж (^X).
hud-xp-tip = Опыт
hud-inventory-tip = Инвентарь (i)
hud-spells-tip = Заклинания (+)
hud-character-tip = Персонаж (^X)
hud-overview-tip = Обзор подземелья (^O)
hud-history-tip = История сообщений (F9)
hud-settings-tip = Настройки
hud-threat-tip = Враг в поле зрения, за краем экрана
hud-ac-tip = Класс брони { $ac }
hud-gold-tip = Золото { $gold }
hud-level = Ур { $level }
hud-hit-dice = КЖ { $hd }
hud-xp = Ур { $level }  ·  { $exp } / { $next }
hud-turn = Ход { $turn }
orb-hp = Здоровье: { $value } из { $most }
orb-pw = Энергия: { $value } из { $most }
minimap-tip = Уровень, насколько вы его знаете. Щелчок: идти туда.
mode-combat-badge = БОЙ — ПОШАГОВО
mode-combat = БОЙ
mode-explore-badge = ИССЛЕДОВАНИЕ
mode-explore = РАЗВЕДКА

## Журнал сообщений

log-title = Сообщения
log-history-title = История сообщений
log-history-close = Закрыть  F9
bar-undo = Отменить
log-count =
    { $n ->
        [one] { $n } сообщение
        [few] { $n } сообщения
       *[many] { $n } сообщений
    }

## Диалоги: вопросы, меню, текстовые окна, палитра команд

dlg-yes = Да (y)
dlg-no = Нет (n)
dlg-cancel-q = Отмена (q)
dlg-all-a = Все (a)
dlg-ok = ОК
dlg-cancel = Отмена
dlg-count = Количество: { $n } — выберите предмет, чтобы взять столько
dlg-selected = Выбрано: { $n }
dlg-hint-read = ↑↓ PgUp PgDn < >: прокрутка · Enter или Esc: закрыть
dlg-space-toggle = Пробел: отметить её
dlg-space-then = затем Пробел отметит её
dlg-hint-any = буква: отметить · ↑↓ выбрать строку, { $space } · '.' все · '-' ни одной · '@' обратить · цифры: количество · PgUp PgDn < >: прокрутка · Enter: ОК · Esc: отмена
dlg-hint-one = буква или щелчок: выбрать · ↑↓ выбрать строку, Enter — взять её · PgUp PgDn < >: прокрутка · Esc: отмена
choice-hint = Esc: отмена
choice-hint-default = { $hint } · Enter: { $answer }
msgmenu-hint-pick = { $letter }: выбрать · Esc: отмена
msgmenu-hint-close = Enter, Пробел или Esc: закрыть
show-hint = ↑↓ PgUp PgDn < >: прокрутка · Enter, Пробел или Esc: закрыть
text-name-placeholder = имя
text-bytes = { $len } / { $max } байт
text-hint = Enter: ОК · Esc: отмена
palette-title = Расширенная команда
palette-placeholder = введите команду
palette-none = нет такой команды
palette-hint = Enter: выполнить выделенную команду · Tab: дополнить · ↑↓ PgUp PgDn: выбор · Esc: отмена
osk-shift = ⇧ Shift
osk-layout = АБВ / ABC
osk-space = Пробел
osk-hint = Крестовина: клавиша · A: ввод · B: стереть · Y: АБВ/ABC · Start: ОК

## Действия с предметами, фильтры и команды панели (ключи nh-world)

item-wield = Взять в руку
item-unwield = Убрать из руки
item-set-alternate = Сделать запасным
item-swap-weapons = Сменить оружие
item-quiver = Положить в колчан
item-empty-quiver = Опустошить колчан
item-fire = Выстрелить
item-throw = Бросить
item-apply = Применить
item-wear = Надеть
item-take-off = Снять
item-put-on = Надеть
item-put-on-left = Надеть на левую руку
item-put-on-right = Надеть на правую руку
item-remove = Снять
item-eat = Съесть
item-quaff = Выпить
item-read = Прочитать
item-zap = Взмахнуть
item-engrave = Писать этим
item-break = Сломать
item-drop = Выбросить
item-drop-some = Выбросить часть…
item-adjust = Сменить букву…
item-split = Разделить стопку…
item-name = Назвать предмет…
item-call = Назвать вид…
item-dip = Окунуть в…
item-two-weapon = Два оружия
item-force = Взломать замок
item-rub = Потереть о…
item-tip = Вытряхнуть
inv-all = Все
inv-suggested = Подходящие
inv-weapons = Оружие
inv-armor = Броня
inv-accessories = Кольца и амулеты
inv-tools = Инструменты
inv-food = Еда
inv-potions = Зелья
inv-scrolls-and-books = Свитки и книги
inv-wands = Жезлы
inv-gems-and-other = Камни и прочее
inv-equipped = Надето
cmd-search = Искать
cmd-rest = Отдыхать до исцеления
cmd-wait = Ждать
cmd-kick = Пнуть
cmd-pick-up = Поднять
cmd-look-here = Осмотреться
cmd-farlook = Рассмотреть
cmd-travel = Идти к…
cmd-pray = Молиться
cmd-offer = Принести жертву
cmd-chat = Поговорить
cmd-loot = Обыскать
cmd-force = Взломать
cmd-sit = Сесть
cmd-turn-undead = Изгнать нежить
cmd-jump = Прыгнуть
cmd-ride = Оседлать
cmd-untrap = Обезвредить
cmd-open = Открыть
cmd-close = Закрыть
cmd-pay = Заплатить
cmd-fire = Выстрелить
cmd-swap = Сменить оружие
cmd-two-weapon = Два оружия
cmd-enhance = Развить навыки
cmd-terrain = Местность
cmd-overview = Обзор
cmd-attributes = Характеристики
cmd-discoveries = Открытия
cmd-cast = Прочесть заклинание
cmd-throw = Бросить
cmd-engrave = Писать на полу
cmd-up = Подняться
cmd-down = Спуститься

## Панель инвентаря

class-weapon = Оружие
class-armor = Броня
class-ring = Кольцо
class-amulet = Амулет
class-tool = Инструмент
class-comestible = Еда
class-potion = Зелье
class-scroll = Свиток
class-spellbook = Книга заклинаний
class-wand = Жезл
class-gem = Камень
class-boulder = Валун или статуя
class-iron-ball = Железное ядро
class-iron-chain = Железная цепь
class-venom = Яд
class-coins = Монеты
class-item = Предмет
fact-blessed = Благословлён
fact-uncursed = Не проклят
fact-cursed = Проклят
fact-enchantment = Зачарование { $value }
fact-containing = Внутри: { $what }
fact-name = Имя: { $name }
fact-called = Вы назвали этот вид: { $name }
doll-helmet = Шлем
doll-cloak = Плащ
doll-body = Доспех
doll-shirt = Рубашка
doll-gloves = Перчатки
doll-boots = Обувь
doll-eyes = Очки и повязки
doll-amulet = Амулет
doll-left-ring = Левое кольцо
doll-right-ring = Правое кольцо
doll-light = Источник света
doll-leash = Поводок
doll-main = Основная рука
doll-off = Вторая рука / щит
doll-alternate = Запасное оружие
doll-quiver = Колчан
doll-main-short = Основное
doll-off-short = Вторая рука
doll-alternate-short = Запасное
doll-quiver-short = Колчан
inv-title = Инвентарь
inv-choose = Выбор
inv-letters = Буквы { $used }/52
inv-letters-tip = Занятые буквы инвентаря (в NetHack их 52)
inv-ac = КБ { $ac }
inv-gold = Золото { $gold }
inv-confirm = Подтвердить  Enter
inv-cancel = Отмена  Esc
inv-close-tip = Закрыть (Esc или i)
inv-suggested-tip = Подходящие (?)
inv-search = Поиск
inv-pack-order = Порядок рюкзака NetHack
inv-detail-empty = Выберите предмет, чтобы узнать, что вам о нём известно.
inv-count-hint = Введите число или используйте ← →.  Enter: ОК · Esc: отмена
inv-count = { $verb }?  { $value } / { $most }
inv-count-how-many = Сколько
inv-count-drop = Сколько выбросить
inv-count-split = Сколько отделить
inv-two-actions = { $action }  (2 действия)
inv-cell-select-tip = Щелчок или { $letter }: { $verb }
inv-cell-menu-tip = Щелчок или его буква: отметить · Shift+щелчок: количество
inv-cell-tip = Двойной щелчок: { $action } ({ $keys }) · Правый щелчок: действия
inv-doll-tip = { $slot }: { $item }
inv-doll-out-tip = Перетащите наружу или дважды щёлкните: { $action } ({ $keys })
inv-doll-empty-tip = { $slot } (пусто) · перетащите сюда предмет
inv-wielding = В руке: { $item }
inv-empty-handed = Руки пусты
inv-pad-filter = { $lb } { $rb }: фильтр
inv-pad-choose = Крестовина: предмет · { $a }: этот · { $b }: отмена
inv-pad-select = Крестовина: предмет · { $a }: выбрать · { $filter } · { $b }: отмена
inv-pad-menu = Крестовина: предмет · { $a }: отметить · { $start }: подтвердить · { $filter } · { $b }: отмена
inv-pad-carrying = Крестовина: куда положить (ячейка куклы, другой предмет) · { $y }: положить · { $b }: вернуть на место
inv-pad-browse =
    Крестовина: предметы и кукла · { $a }: первое действие · { $x }: все действия
    { $y }: взять, затем снова { $y } там, куда положить (на куклу — надеть) · { $filter } · { $b }: закрыть
inv-nothing-suggested = Подходящих нет — можно выбрать любой предмет. { $hint }
inv-split-to = Отделить { $n } от { $from } — на какую букву?
inv-adjust-to = Сменить букву { $from } — на какую?
inv-adjust-hint = Введите новую букву или щёлкните предмет, с которым поменять. Esc: отмена
inv-dip-into = Окунуть { $from } во что?
inv-dip-hint = Щёлкните предмет, в который окунуть, или введите его букву. Esc: отмена
inv-count-typed = Количество { $n }
inv-select-hint = Щёлкните предмет или нажмите его букву · ? подходящие · * все · Esc: отмена
inv-select-hint-count = { $hint } · цифры или Shift+щелчок: количество
inv-menu-any-hint =
    Щелчок или буква: отметить · Shift+щелчок или цифры: количество
    '.' все · '-' ни одного · '@' обратить · Enter: подтвердить · Esc: отмена
inv-menu-one-hint = Щёлкните или нажмите букву, чтобы выбрать · Esc: отмена
inv-carrying = Предмет в руках
inv-carrying-hint = Наведите туда, куда положить (ячейка куклы, другой предмет), и снова нажмите Y · B: вернуть на место
inv-browse-hint =
    Перетащите на куклу — надеть · на панель действий — назначить · за панель — выбросить
    Двойной щелчок: первое действие · Правый щелчок: все действия
inv-your-pack = Ваш рюкзак
inv-show-only = Показать только: { $what }
inv-class-line = { $class } · буква { $letter }
inv-raw = «{ $text }»
hands-bare = Голые руки
hands-fingers = Пальцы
hands-empty-quiver = Ничего (опустошить колчан)
hands-nothing = Ничего
fact-recharged =
    Перезаряжен { $times } { $times ->
        [one] раз
        [few] раза
       *[many] раз
    } · { $charges ->
        [one] остался { $charges } заряд
        [few] осталось { $charges } заряда
       *[many] осталось { $charges } зарядов
    }
fact-charges =
    { $n ->
        [one] { $n } заряд
        [few] { $n } заряда
       *[many] { $n } зарядов
    }

## Геймпад: полоса подсказок, круговое меню

hint-act = Действие
hint-search = Искать
hint-inventory = Инвентарь
hint-actions = Действия
hint-fire = Выстрел
hint-bar = Панель
hint-commands = Команды
hint-pick = Выбрать
hint-cancel = Отмена
hint-here = Здесь
hint-toggle = Отметить
hint-confirm = Подтвердить
hint-page = Страница
hint-use = Использовать
hint-carry = Взять / положить
hint-filter = Фильтр
hint-close = Закрыть
hint-choose = Выбрать
hint-type = Ввод
hint-erase = Стереть
hint-layout = АБВ / ABC
hint-ok = ОК
hint-back = Назад
radial-title = Действия
radial-hint = Наведите стик на действие · отпустите в центре: ничего
radial-let-go = Отпустите LT, чтобы выполнить
radial-here = Действия здесь
radial-pick-up = Поднять
radial-fight = Атаковать
radial-kick = Пнуть
radial-rest = Отдых
radial-pray = Молиться
radial-travel = Идти к…
radial-save = Сохранить

## Панель действий, ещё

bar-slot-empty = Ячейка { $key } (пусто)

## Выбор названий: желание, чудовище, класс (ввод по-русски)

picker-none = Ничего не найдено
picker-more = Ещё { $n }: уточните поиск
picker-class = Класс
picker-class-all = Все классы
picker-class-weapon = Оружие
picker-class-armor = Доспехи
picker-class-ring = Кольца
picker-class-amulet = Амулеты
picker-class-tool = Инструменты
picker-class-food = Еда
picker-class-potion = Зелья
picker-class-scroll = Свитки
picker-class-spellbook = Книги заклинаний
picker-class-wand = Жезлы
picker-class-coin = Монеты
picker-class-gem = Самоцветы и камни
picker-class-heavy = Валуны, статуи, железо
picker-placeholder-wish = Например: благословенный +2 длинный меч
picker-placeholder-monster = Название чудовища
picker-placeholder-class = Класс или одно из его чудовищ
picker-placeholder-write = Что написать
picker-hint-wish = Пишите желание: количество, благословение, +зачарование, предмет · Enter: загадать · ↑↓: выбор · Esc: отмена
picker-hint = Enter: выбрать · ↑↓: выбор · Esc: отмена
picker-wish = Загадать
picker-choose = Выбрать
picker-manual = Ввести по-английски
wish-count = Количество
wish-ench = Зачарование
wish-buc-any = Благословение: любое
wish-buc-blessed = Благословенный
wish-buc-uncursed = Непроклятый
wish-buc-cursed = Проклятый
wish-shown = Желание: { $wish }
wish-pick = Выберите предмет в списке
text-latin = Будет написано латиницей: { $text }
hint-wish = Загадать
hint-count = Количество
hint-blessing = Благословение
hint-enchantment = Зачарование
hint-class = Класс
hint-english = По-английски

## Достижения

title-achievements = Достижения
hud-achievements-tip = Достижения
achievements-title = Достижения
achievements-progress = Получено { $earned } из { $total }
achievements-unlocked = Новое достижение
achievements-hidden-name = Скрытое достижение
achievements-hidden-desc = Играйте дальше, чтобы узнать, какое.
achievements-locked = Ещё не получено
achievements-earned = Получено: { $character }, ход { $turn }, { $date }
achievements-hint = Стрелки: выбор · Esc: назад
achievements-back = Назад

achievement-bell-name = Позвони мне
achievement-bell-desc = Отнимите Колокол Открытия у заклятого врага вашего Квеста.
achievement-gehennom-name = Оставь надежду
achievement-gehennom-desc = Войдите в Геенну через Долину Мёртвых.
achievement-candelabrum-name = Семь свечей
achievement-candelabrum-desc = Добудьте Канделябр Призыва в Башне Влада.
achievement-book-of-the-dead-name = Обязательное чтение
achievement-book-of-the-dead-desc = Отнимите Книгу Мёртвых у Волшебника Йендора.
achievement-invocation-name = Врата открываются
achievement-invocation-desc = Совершите Призыв на вибрирующем квадрате.
achievement-amulet-name = Главный приз
achievement-amulet-desc = Отнимите Амулет Йендора у верховного жреца Молоха.
achievement-planes-name = За пределами подземелья
achievement-planes-desc = Поднимитесь с Амулетом в Стихийные планы.
achievement-astral-name = Среди звёзд
achievement-astral-desc = Доберитесь до Астрального плана.
achievement-ascension-name = Полубог
achievement-ascension-desc = Принесите Амулет Йендора в дар своему богу и вознеситесь.
achievement-mines-prize-name = Счастливый камень
achievement-mines-prize-desc = Найдите камень удачи, спрятанный на Дне копей.
achievement-sokoban-prize-name = Головоломка решена
achievement-sokoban-prize-desc = Заберите награду на вершине Сокобана.
achievement-medusa-name = Не смотри в глаза
achievement-medusa-desc = Убейте Медузу.
achievement-blind-name = Во тьме
achievement-blind-desc = Проведите всю игру вслепую, от первого хода до последнего.
achievement-nudist-name = Нечего надеть
achievement-nudist-desc = Закончите игру, ни разу не надев доспехов.
achievement-mines-name = Гномьи копи
achievement-mines-desc = Спуститесь в Гномьи копи.
achievement-minetown-name = Шахтёрский городок
achievement-minetown-desc = Доберитесь до Шахтёрского городка.
achievement-shop-name = Покупатель
achievement-shop-desc = Зайдите в лавку.
achievement-temple-name = Святилище
achievement-temple-desc = Войдите в храм.
achievement-oracle-name = Мудрое слово
achievement-oracle-desc = Обратитесь к Оракулу.
achievement-novel-name = Хорошая книга
achievement-novel-desc = Прочтите отрывок из романа о Плоском мире.
achievement-sokoban-name = Сокобан
achievement-sokoban-desc = Войдите в Сокобан.
achievement-big-room-name = Большой зал
achievement-big-room-desc = Войдите в Большой зал.
achievement-rank-1-name = На подъёме
achievement-rank-1-desc = Достигните 3-го уровня опыта и нового звания своей роли.
achievement-rank-2-name = Боевой опыт
achievement-rank-2-desc = Достигните 6-го уровня опыта и нового звания своей роли.
achievement-rank-3-name = Ветеран
achievement-rank-3-desc = Достигните 10-го уровня опыта и нового звания своей роли.
achievement-rank-4-name = Закалка
achievement-rank-4-desc = Достигните 14-го уровня опыта и нового звания своей роли.
achievement-rank-5-name = Громкое имя
achievement-rank-5-desc = Достигните 18-го уровня опыта и нового звания своей роли.
achievement-rank-6-name = Слава
achievement-rank-6-desc = Достигните 22-го уровня опыта и нового звания своей роли.
achievement-rank-7-name = Живая легенда
achievement-rank-7-desc = Достигните 26-го уровня опыта и нового звания своей роли.
achievement-rank-8-name = Без равных
achievement-rank-8-desc = Достигните 30-го уровня опыта и нового звания своей роли.
achievement-tune-name = Пять нот
achievement-tune-desc = Узнайте мелодию, которая открывает подъёмный мост Замка.
achievement-quest-called-name = Зов
achievement-quest-called-desc = Получите от своего наставника призыв к Квесту.
achievement-quest-done-name = Квест выполнен
achievement-quest-done-desc = Принесите артефакт Квеста своему наставнику.
achievement-crowned-name = Коронация
achievement-crowned-desc = Удостойтесь короны от своего бога.
achievement-wizard-name = Гроза волшебников
achievement-wizard-desc = Убейте Волшебника Йендора.
achievement-drawbridge-name = Сезам, откройся
achievement-drawbridge-desc = Откройте подъёмный мост Замка.
achievement-vibrating-square-name = Хорошие вибрации
achievement-vibrating-square-desc = Найдите вибрирующий квадрат.
achievement-major-oracle-name = Большое прорицание
achievement-major-oracle-desc = Заплатите Оракулу за большое прорицание.
achievement-amulet-wish-name = Загадай желание
achievement-amulet-wish-desc = Получите желание от Амулета Йендора.
achievement-depth-10-name = В глубине
achievement-depth-10-desc = Спуститесь на 10-й уровень подземелья.
achievement-depth-20-name = Искатель глубин
achievement-depth-20-desc = Спуститесь на 20-й уровень подземелья.
achievement-depth-30-name = Подземный мир
achievement-depth-30-desc = Спуститесь на 30-й уровень подземелья.
achievement-depth-40-name = Бездна отчаяния
achievement-depth-40-desc = Спуститесь на 40-й уровень подземелья.
achievement-ascend-arc-name = Вознесение археолога
achievement-ascend-arc-desc = Вознеситесь, играя археологом.
achievement-ascend-bar-name = Вознесение варвара
achievement-ascend-bar-desc = Вознеситесь, играя варваром.
achievement-ascend-cav-name = Вознесение пещерного человека
achievement-ascend-cav-desc = Вознеситесь, играя пещерным человеком.
achievement-ascend-hea-name = Вознесение целителя
achievement-ascend-hea-desc = Вознеситесь, играя целителем.
achievement-ascend-kni-name = Вознесение рыцаря
achievement-ascend-kni-desc = Вознеситесь, играя рыцарем.
achievement-ascend-mon-name = Вознесение монаха
achievement-ascend-mon-desc = Вознеситесь, играя монахом.
achievement-ascend-pri-name = Вознесение жреца
achievement-ascend-pri-desc = Вознеситесь, играя жрецом или жрицей.
achievement-ascend-rog-name = Вознесение плута
achievement-ascend-rog-desc = Вознеситесь, играя плутом.
achievement-ascend-ran-name = Вознесение следопыта
achievement-ascend-ran-desc = Вознеситесь, играя следопытом.
achievement-ascend-sam-name = Вознесение самурая
achievement-ascend-sam-desc = Вознеситесь, играя самураем.
achievement-ascend-tou-name = Вознесение туриста
achievement-ascend-tou-desc = Вознеситесь, играя туристом.
achievement-ascend-val-name = Вознесение валькирии
achievement-ascend-val-desc = Вознеситесь, играя валькирией.
achievement-ascend-wiz-name = Вознесение волшебника
achievement-ascend-wiz-desc = Вознеситесь, играя волшебником.
achievement-ascend-vegan-name = Веганское вознесение
achievement-ascend-vegan-desc = Вознеситесь, не съев ничего животного происхождения.
achievement-ascend-vegetarian-name = Вегетарианское вознесение
achievement-ascend-vegetarian-desc = Вознеситесь, не съев ни одного животного.
achievement-ascend-foodless-name = Вознесение без еды
achievement-ascend-foodless-desc = Вознеситесь, вообще ничего не съев.
achievement-ascend-atheist-name = Атеистическое вознесение
achievement-ascend-atheist-desc = Вознеситесь, ни разу не помолившись, не воспользовавшись алтарём и не обратившись к жрецу.
achievement-ascend-weaponless-name = Вознесение без оружия
achievement-ascend-weaponless-desc = Вознеситесь, ни разу не ударив оружием в руках.
achievement-ascend-pacifist-name = Вознесение пацифиста
achievement-ascend-pacifist-desc = Вознеситесь, не убив своими руками ни одного монстра.
achievement-ascend-illiterate-name = Неграмотное вознесение
achievement-ascend-illiterate-desc = Вознеситесь, ничего не прочитав.
achievement-ascend-polypileless-name = Вознесение без превращений вещей
achievement-ascend-polypileless-desc = Вознеситесь, не превратив ни одного предмета.
achievement-ascend-polyselfless-name = Вознесение в своём облике
achievement-ascend-polyselfless-desc = Вознеситесь, ни разу не сменив облик.
achievement-ascend-wishless-name = Вознесение без желаний
achievement-ascend-wishless-desc = Вознеситесь, не загадав ни одного желания.
achievement-ascend-artiwishless-name = Вознесение без желанных артефактов
achievement-ascend-artiwishless-desc = Вознеситесь, не пожелав ни одного артефакта.
achievement-ascend-petless-name = Вознесение без питомца
achievement-ascend-petless-desc = Вознеситесь, ни разу не заведя питомца.
achievement-sokoban-purist-name = Честный Сокобан
achievement-sokoban-purist-desc = Заберите награду Сокобана, ни разу не нарушив его правил.

## Справка: руководство по NetHack

hud-help-tip = Справка: руководство (F1)
help-title = Справка — { $book }
help-close-tip = Закрыть (Esc, F1)
help-search-placeholder = Поиск по руководству
help-nothing = Ничего не найдено
help-hint = ↑↓: глава · PgUp PgDn: прокрутка · Esc: закрыть
hint-chapter = Глава
hint-scroll = Прокрутка

## Состояние: уровень подземелья, опыт и очки

hud-dlvl = Глубина { $level }
hud-tutorial-level = Обучение { $level }
hud-exp = Опыт { $exp }
hud-score = Очки { $score }

## Картины и таблицы игры, которые раскладывает клиент: надгробие,
## #vanquished, #genocided, #overview

rip-rest-in-peace =
    ПОКОЙСЯ
    С
    МИРОМ
rip-gold = { $gold } Au
vanquished-title = Побеждённые существа
vanquished-rider = Всадник
vanquished-total =
    { $count ->
        [one] Побеждено { $count } существо
        [few] Побеждено { $count } существа
       *[many] Побеждено { $count } существ
    }
genocided-title = Истреблённые виды
genocided-title-extinct = Вымершие виды
genocided-title-both = Истреблённые и вымершие виды
genocided-extinct = вымер
genocided-total = Истреблено видов: { $count }
extinct-total = Вымерло видов: { $count }
overview-title = Обзор подземелья
overview-levels = уровни { $from }–{ $to }
overview-levels-up = уровни с { $from } вверх до { $to }
overview-level = Уровень { $level }
overview-astral = Астральный план
overview-plane-earth = План Земли
overview-plane-air = План Воздуха
overview-plane-fire = План Огня
overview-plane-water = План Воды
overview-here = Вы здесь
overview-left-from = Отсюда вы ушли
overview-were = Вы были здесь
overview-note = «{ $note }»
overview-shops =
    { $seen ->
        [two] 2 лавки
       *[many] много лавок
    }
overview-temples =
    { $seen ->
        [one] храм
        [two] 2 храма
       *[many] много храмов
    }
overview-temples-to =
    { $seen ->
        [one] храм { $god }
        [two] 2 храма { $god }
       *[many] много храмов { $god }
    }
overview-altars =
    { $seen ->
        [one] алтарь
        [two] 2 алтаря
       *[many] много алтарей
    }
overview-altars-to =
    { $seen ->
        [one] алтарь { $god }
        [two] 2 алтаря { $god }
       *[many] много алтарей { $god }
    }
overview-thrones =
    { $seen ->
        [one] трон
        [two] 2 трона
       *[many] много тронов
    }
overview-fountains =
    { $seen ->
        [one] фонтан
        [two] 2 фонтана
       *[many] много фонтанов
    }
overview-sinks =
    { $seen ->
        [one] раковина
        [two] 2 раковины
       *[many] много раковин
    }
overview-graves =
    { $seen ->
        [one] могила
        [two] 2 могилы
       *[many] много могил
    }
overview-trees =
    { $seen ->
        [one] дерево
        [two] 2 дерева
       *[many] много деревьев
    }
overview-oracle = Дельфийский оракул
overview-sokoban-solved = Решён
overview-sokoban-unsolved = Не решён
overview-bigroom = Очень большой зал
overview-rogue = Первобытный край
overview-home = Дом
overview-home-lost = Дом (пути назад нет…)
overview-quest-done = Выполнено задание { $leader }
overview-quest-given = Задание получено от { $leader }
overview-summoned = Призыв от { $leader }
overview-ludios = Форт Лудиос
overview-castle = Замок
overview-castle-notes = Замок: сыграйте { $notes }, чтобы опустить или поднять подъёмный мост
overview-castle-tune = Замок: сыграйте мелодию из пяти нот, чтобы опустить или поднять подъёмный мост
overview-valley = Долина Мёртвых
overview-gateway = Врата в Святилище Молоха
overview-sanctum = Святилище Молоха
overview-stairs-up = Лестница вверх в { $place }
overview-stairs-down = Лестница вниз в { $place }
overview-one-way-up = Лестница в один конец вверх в { $place }
overview-one-way-down = Лестница в один конец вниз в { $place }
overview-portal = Портал в { $place }
overview-sealed-portal = Закрытый портал в { $place }
overview-connection = Проход в { $place }
overview-unknown-way = Путь в { $place }
overview-branch-level = { $branch }, уровень { $level }
overview-resting = Здесь покоятся
overview-resting-you = Здесь покоитесь вы
overview-dead-you = { $how }
overview-dead = { $who }, { $how }
