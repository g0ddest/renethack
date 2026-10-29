# renethack: собственный интерфейс клиента, по-русски (ключи те же, что
# в en.ftl). Слова движка (сообщения, названия, меню) здесь не
# переводятся: их переводит nh-i18n.

## Заставка, настройки, создание персонажа, конец игры, ошибки

veil-first-launch = Подготовка к первому запуску…
title-subtitle = NetHack 5.0
title-new-game = Новая игра
title-continue = Продолжить: { $name } ({ $time })
title-settings = Настройки
title-credits = Авторы
title-quit = Выход

settings-title = Настройки
settings-back = Назад
settings-credits = Авторы и лицензии
settings-hint = ↑↓: выбор · ←→: изменить · Tab: раздел · Esc: назад
settings-display = Экран
settings-interface = Интерфейс
settings-on = Вкл.
settings-off = Выкл.
settings-window-mode = Окно
settings-window-windowed = В окне
settings-window-borderless = Полный экран без рамки
settings-window-exclusive = Эксклюзивный полный экран
settings-window-mode-about = Окно или весь экран: без рамки рабочий стол остаётся в одном нажатии, эксклюзивный отдаёт экран одной игре.
settings-window-size = Размер окна
settings-size = { $width } × { $height }
settings-window-size-about = Размер окна — из тех, что помещаются на экране.
settings-window-size-fullscreen = Полный экран занимает весь экран: размер окна выбирается в окне.
settings-vsync = Вертикальная синхронизация
settings-vsync-about = Кадры выводятся в такт с экраном, без разрывов. Без неё — так быстро, как рисуются, но не чаще ограничения.
settings-fps-cap = Ограничение кадров
settings-fps-none = Нет
settings-fps = { $fps } к/с
settings-fps-cap-about = Предел кадров в секунду: компьютер меньше греется и тише шумит, батарея живёт дольше.
settings-graphics = Графика
settings-quality = Качество графики
settings-quality-low = Низкое
settings-quality-medium = Среднее
settings-quality-high = Высокое
settings-quality-ultra = Ультра
settings-quality-custom = Своё
settings-quality-about = Набор задаёт все настройки ниже; если изменить одну из них вручную, качество станет своим.
settings-ssao = Затенение (SSAO)
settings-ssao-about = Тень там, где стены сходятся с полом, и в углах: камень обретает глубину.
settings-ssil = Непрямое освещение (SSIL)
settings-ssil-about = Свет факелов, отражённый от стен и пола: тёплый отсвет вокруг огня.
settings-fog = Объёмный туман
settings-fog-about = Дымка, что светится вокруг огня и висит во тьме.
settings-shadows = Качество теней
settings-shadows-about = Чёткость и мягкость теней и сколько ближайших к герою факелов их отбрасывают.
settings-msaa = Сглаживание (MSAA)
settings-samples = { $samples }×
settings-msaa-about = Сглаживает края стен и моделей; каждая ступень дороже для видеокарты.
settings-scale = Разрешение 3D
settings-percent = { $percent } %
settings-scale-about = 3D-вид рисуется в части разрешения окна и растягивается до него: быстрее, чуть мягче. Интерфейс остаётся чётким.
settings-upscale = Масштабирование
settings-upscale-bilinear = Билинейное
settings-upscale-fsr1 = AMD FSR 1.0
settings-upscale-fsr2 = AMD FSR 2.2
settings-upscale-about = Как 3D-вид в разрешении ниже окна растягивается до него. FSR 2.2 к тому же сглаживает края от кадра к кадру, даже при 100 %.
settings-sound = Звук
settings-volume-master = Общая громкость
settings-volume-master-about = Все звуки игры сразу.
settings-volume-music = Музыка
settings-volume-music-about = Музыка подземелья и его ветвей.
settings-volume-ambience = Окружение
settings-volume-ambience-about = Собственные звуки подземелья: ветер, вода, огонь, далёкое эхо.
settings-volume-sfx = Эффекты
settings-volume-sfx-about = Удары, заклинания, двери, шаги и голоса существ; и то, что герой только слышит.
settings-volume-ui = Звуки интерфейса
settings-volume-ui-about = Щелчки кнопок и панелей.
settings-ui-scale = Масштаб интерфейса
settings-ui-scale-auto = Авто ({ $percent } %)
settings-ui-scale-about = Размер надписей и панелей. Авто: 120 % на Steam Deck, иначе 100 %.
settings-ui-scale-forced = Задан переменной RENETHACK_UI_SCALE на этот запуск.
settings-keys = Клавиши новых персонажей
settings-keys-modern = Современные
settings-keys-classic = Классические
settings-keys-about = { $keys }. Персонаж сохраняет клавиши, с которыми был создан.
settings-keys-playing-modern = Этот персонаж играет на современных клавишах.
settings-keys-playing-classic = Этот персонаж играет на классических клавишах.
settings-language = Язык
settings-language-about = Язык интерфейса и текстов игры.
settings-reduce-effects = Меньше эффектов на экране
settings-reduce-effects-about = Без покачивания героя, без дрейфа камеры, без переливов цвета при галлюцинациях. Слепота по-прежнему затемняет вид.
settings-hints = Подсказки
settings-hints-about = Карточка рядом с интерфейсом, когда что-то встречается впервые: существо, вещь под ногами, голод, лестница… Путеводитель в справке хранит увиденные.
settings-order-speed = Скорость приказов
settings-order-speed-about = Как быстро идут ваши приказы — путь по щелчку, зажатая клавиша, отдых по F5: шаг сразу (мгновенно), каждые 0,15 с (быстро), 0,3 с (обычно) или 0,5 с (медленно). Повторы по числу перед командой — дело самого NetHack, они идут сразу.
settings-order-speed-instant = Мгновенно
settings-order-speed-fast = Быстро
settings-order-speed-normal = Обычно
settings-order-speed-slow = Медленно
settings-stairs-walk = < и > ведут к лестнице
settings-stairs-walk-about = Когда вы не на лестнице: «Вкл.» — герой идёт к известной лестнице и проходит по ней; «Выкл.» — ответ как в NetHack («Здесь нельзя спуститься»).
settings-gameplay = Игра
settings-autopickup = Автоподбор
settings-autopickup-about = Шаг на предметы подбирает те их виды, что выбраны строкой ниже.
settings-pickup-types = Что подбирать
settings-pickup-all = Всё
settings-pickup-coins = Монеты
settings-pickup-valuables = Монеты и мелкие ценности
settings-pickup-custom = Классы { $symbols }
settings-pickup-types-about = Что берёт автоподбор: всё, только монеты или монеты с амулетами, свитками, книгами заклинаний, зельями, кольцами и палочками.
settings-pickup-burden = Предел ноши без вопроса
settings-burden-unencumbered = Без нагрузки
settings-burden-burdened = Нагрузка
settings-burden-stressed = Перегрузка
settings-burden-strained = Сильная перегрузка
settings-burden-overtaxed = Тяжкая перегрузка
settings-burden-overloaded = Неподъёмный груз
settings-pickup-burden-about = Если поднятое нагрузит героя сверх этого, игра сначала спросит.
settings-autoopen = Открывать двери шагом
settings-autoopen-about = Шаг в закрытую дверь открывает её.
settings-autoquiver = Наполнять колчан для стрельбы
settings-autoquiver-about = Выстрел с пустым колчаном сначала кладёт в него то, чем можно стрелять.
settings-safe-pet = Щадить питомцев и мирных
settings-safe-pet-about = Шаг на питомца или мирное существо в поле зрения меняет вас местами или останавливает, а не бьёт; F бьёт всё равно.
settings-confirm = Спрашивать перед ударом мирных
settings-confirm-about = Нападение на мирное существо сначала спрашивает.
settings-paranoid-attack = «yes» для удара мирных
settings-paranoid-attack-about = На этот вопрос нужно ответить словом yes, а не просто y.
settings-paranoid-pray = Спрашивать перед молитвой
settings-paranoid-pray-about = Молитва сначала спрашивает: слишком ранняя молитва гневит богов.
settings-paranoid-quit = «yes» для выхода без сохранения
settings-paranoid-quit-about = Выход без сохранения или переход в режим исследования требует слова yes, а не просто y.
settings-paranoid-remove = Всегда спрашивать, что снять
settings-paranoid-remove-about = Команды «Снять» спрашивают, какую вещь, даже если надета одна.
settings-verbose = Подробные сообщения
settings-verbose-about = Больше пояснений по ходу игры; без них сообщения говорят только главное.
settings-gameplay-new = Так начнутся новые игры; у сохранённой игры свои.
settings-gameplay-game = Свои у этой игры, как она их сообщает; так начнутся и новые игры.
settings-gameplay-waiting = Игра их ещё не сообщила.
credits-title = Авторы и лицензии
credits-back = Назад
credits-hint = ↑↓: выбор · PgUp PgDn: прокрутка · Esc: назад
credits-group-credits = Авторы
credits-group-licences = Лицензии
credits-game = renethack
credits-nethack = NetHack
credits-art = Графика
credits-sound = Звук
credits-help = Руководство в справке
credits-translation = Русский перевод
credits-notice = Что в комплекте и на каких условиях
credits-ngpl = NetHack General Public License
credits-godot = Godot Engine
credits-godot-parts = Сторонний код Godot
credits-rust = Библиотеки Rust
credits-engine-parts = Lua и cJSON
credits-fonts = Шрифты
credits-steam = Steamworks
credits-game-text = Клиент NetHack 5.0, рисующий подземелье в трёх измерениях; сама игра — NetHack, такая как есть.
credits-game-by = Автор: { $author }.
credits-nethack-text = Игра, в которую играет клиент: NetHack 5.0 от NetHack DevTeam, с патчами renethack и его протокольным хостом.
credits-nethack-licence = NetHack распространяется по NetHack General Public License (см. лицензии).
credits-ngpl-missing = Копия лицензии из движка не найдена: см. NetHack-General-Public-License.txt в папке legal игры.
credits-lua-missing = Полное уведомление Lua — в Lua-LICENSE.txt в папке legal игры.
credits-godot-parts-text = Godot Engine { $version } включает следующий сторонний код.
credits-rust-text = Расширение написано на Rust и включает эти библиотеки; тексты их лицензий — ниже.
credits-fonts-text = Шрифты интерфейса распространяются по SIL Open Font License 1.1.
credits-steam-text = Эта сборка включает библиотеку Steamworks API от Valve (steam_api), распространяемую по Steamworks SDK Access Agreement.
settings-controls = Управление
controls-show = Показать
controls-shown-modern = Клавиатура: современные клавиши
controls-shown-classic = Клавиатура: классические клавиши
controls-shown-pad = Геймпад
controls-show-about = Что делают клавиши и геймпад; ↓ ведёт по списку. Персонаж сохраняет клавиши, с которыми был создан.
controls-group-movement = Движение
controls-group-interface = Интерфейс
controls-group-commands = Команды NetHack
controls-group-fixed = Клавиши, которые не меняются
controls-step = Шаг
controls-zoom-in = Приблизить
controls-zoom-out = Отдалить
controls-count-prefix = Число повторов перед командой
controls-slot = Ячейка панели действий { $n }
controls-keypad-moves = Цифровой блок: шаги, а на 5 — отдых
controls-fixed-escape = Esc отменяет и возвращает назад, везде
controls-fixed-keypad = шаги цифрового блока
controls-fixed-run = бег: Shift и шаг
controls-chord = Шаг по диагонали; с Shift — бег
controls-chord-note = Нет цифровой клавиатуры? Нажмите две стрелки вместе, чтобы шагнуть по диагонали.
controls-fixed-arrows = шаги стрелками, рядом с буквами
controls-fixed-count = число повторов: цифры верхнего ряда, как в NetHack
controls-fixed-alternate = вторая клавиша команды в самом NetHack
controls-fixed = { $key } не меняется ({ $what }): нажмите другую клавишу или Esc.
controls-taken = { $key } — клавиша команды «{ $name }». Enter: поменять их местами · Esc: отмена
controls-press = Нажмите новую клавишу для «{ $name }» · Esc: отмена
controls-edit-about = Enter или щелчок: другая клавиша · Delete: вернуть её собственную. Только там, где NetHack ждёт команду: в меню, вопросах и тексте клавиша остаётся собой.
controls-own-key = своя клавиша: { $key }
controls-reset-all = Все клавиши — на свои команды
controls-reset-all-about = Enter: вернуть все клавиши этого набора, как в NetHack.
controls-reset-all-confirm = Enter ещё раз, чтобы вернуть все клавиши · Esc: отмена
controls-keypad = цифровой блок
controls-direction = направление
controls-left-click = левый щелчок по карте
controls-right-click = правый щелчок по карте
controls-any-key = любая клавиша
controls-wheel = колесо мыши
controls-left-stick = левый стик
controls-right-stick = правый стик
controls-dpad = крестовина
controls-run = Бег
controls-count = Число повторов: двадцать поисков
controls-inventory = Инвентарь
controls-palette = Палитра команд
controls-click = Идти туда; взять, открыть или атаковать то, что там
controls-right-click-does = Что там можно сделать
controls-rest = Отдых, пока здоровье и энергия не восстановятся
controls-stop = Остановить приказ до следующего шага
controls-overview = Весь известный уровень; ещё раз — обратно к герою
controls-zoom = Отдалить, приблизить
controls-log = Журнал сообщений
controls-help = Справка: руководство NetHack
controls-pad-walk = Идти в восьми направлениях; в меню — переводить выбор
controls-pad-cursor = Курсор на карте
controls-pad-a = Действие с клеткой под курсором; выбрать
controls-pad-b = Назад, отмена
controls-pad-x = Искать; действия с предметом
controls-pad-y = Инвентарь; перенести предмет на место
controls-pad-bar-first = Ячейки панели действий 1–4
controls-pad-bar-second = Ячейки панели действий 5–8
controls-pad-bar-page = Другая страница панели: ячейки 9 и 0
controls-pad-bumpers = В меню и инвентаре: страница или фильтр до, после
controls-pad-radial = Удерживать: круг действий
controls-pad-fire = Выстрел
controls-pad-start = Палитра команд; подтвердить
controls-pad-history = История сообщений
controls-pad-sticks = Весь уровень; курсор обратно на героя

creation-title = Новый персонаж
creation-name = Имя
creation-role = Роль
creation-race = Раса
creation-gender = Пол
creation-alignment = Мировоззрение
creation-keys = Клавиши
creation-keys-modern = Современные: ходьба стрелками или цифровым блоком
creation-keys-classic = Классические: клавиши NetHack, ходьба буквами hjklyubn
creation-keys-tip = Современные: ходьба стрелками или цифровым блоком, остальное — буквами (k — пинок, j — прыжок, l — обыскать), n и число повторяют команду, 1–0 — панель действий. Классические: клавиши NetHack (ходьба — hjklyubn), число перед командой повторяет её, Alt+1–0 — панель действий.
creation-first-game-key = — хорошо для первой игры
creation-keys-modern-name = Современные
creation-keys-classic-name = Классические
creation-keys-modern-line = Ходьба стрелками или цифровым блоком, остальное — буквами.
creation-keys-classic-line = Клавиши NetHack: ходьба буквами hjklyubn.
creation-language = Язык
creation-random = Случайно
creation-start = Начать
creation-back = Назад
creation-continue-instead = Продолжить { $name }
creation-name-taken = Есть сохранённая игра с этим именем: начав, вы продолжите её.
creation-name-placeholder = Пусто — выберет жребий
creation-name-roll-tip = Случайное имя, подходящее герою
creation-name-bad = В имени могут быть только буквы, цифры, пробелы и ' _ .
# имена жребия: героини, героя (не длиннее 31 байта: буква кириллицы — два)
creation-names-female = Агния, Белослава, Василиса, Веденея, Велена, Веселина, Голуба, Дарина, Добрава, Забава, Злата, Купава, Лада, Любава, Милава, Мирослава, Огняна, Олеся, Радмила, Ростислава, Светозара, Снежана, Умила, Ярослава
creation-names-male = Богдан, Борислав, Будимир, Вадим, Ведислав, Велимир, Всеволод, Вышеслав, Гордей, Добрыня, Ждан, Звенислав, Изяслав, Любомир, Милорад, Мстислав, Олег, Остромир, Радим, Ратибор, Святослав, Тихомир, Яромир, Ярополк
creation-first-game = Для первой игры
creation-about-random = Роль выпадет при старте игры.
creation-about-arc = Скорость, скрытность и кирка, которой можно прорыть стены и пол. Бой — не сильная сторона.
creation-about-bar = Сила, колечный доспех, двуручный меч или топор и невосприимчивость к яду.
creation-about-cav = Выносливость, дубина и праща: просто и крепко, но к магии способностей мало.
creation-about-hea = Скальпель — слабое оружие, зато много золота, невосприимчивость к яду, лечащие заклинания и палочка сна.
creation-about-kni = Крепкие доспехи, рыцарское копьё и пони для верховой езды. Кодекс чести запрещает бить бегущего врага.
creation-about-mon = Бой голыми руками, скорость и увёртливость. Доспехи и мясо идут вразрез с учением монахов.
creation-about-pri = С первого взгляда видно, благословлён предмет или проклят. Булава, щит и заклинания жречества.
creation-about-rog = Скрытность и кинжалы, которые можно метать по нескольку сразу. Открытый бой опасен.
creation-about-ran = Лук и стрелы, выпущенные залпом: врагов лучше держать на расстоянии.
creation-about-sam = Скорость и мастерство: катана, короткий меч и наборный доспех.
creation-about-tou = Самый трудный старт: слабость, дротики, фотоаппарат, свитки магического картирования и много золота.
creation-about-val = Выносливость, копьё и щит, скрытность и сопротивление холоду: самый лёгкий старт.
creation-about-wiz = Силовой удар с самого начала, магические предметы и плащ сопротивления магии. Вблизи — уязвимость.

end-title = Игра окончена
end-title-ascended = Вознесение
end-title-escaped = Побег из подземелья
end-new-game = Новая игра
end-to-title = В меню
end-quit = Выход
end-hint = ↑↓ PgUp PgDn: прокрутка · ←→ Tab: выбор · Enter: нажать · Esc: в меню
end-stone-quit =
    { $female ->
        [yes] сдалась
       *[other] сдался
    }
end-ascended =
    { $female ->
        [yes] Вознеслась и стала полубогиней
       *[other] Вознёсся и стал полубогом
    }
end-escaped =
    { $female ->
        [yes] Выбралась из подземелья живой
       *[other] Выбрался из подземелья живым
    }
end-stat-turns = Ходы
end-stat-deepest = Глубина
end-stat-score = Очки
end-stat-place = Место
end-stat-gold = Золото
end-stat-level = Уровень
end-achievements = Достижения
end-achievements-none = В этой игре — ни одного.
end-achievement-new = Новое
end-conduct = Выдержанные испытания
end-conduct-none = Ни одного.
end-conduct-foodless = Без еды
end-conduct-vegan =
    { $female ->
        [yes] Веганка
       *[other] Веган
    }
end-conduct-vegetarian =
    { $female ->
        [yes] Вегетарианка
       *[other] Вегетарианец
    }
end-conduct-atheist =
    { $female ->
        [yes] Атеистка
       *[other] Атеист
    }
end-conduct-weaponless = Без оружия
end-conduct-pacifist =
    { $female ->
        [yes] Пацифистка
       *[other] Пацифист
    }
end-conduct-illiterate =
    { $female ->
        [yes] Неграмотная
       *[other] Неграмотный
    }
end-conduct-polypileless = Без превращения предметов
end-conduct-polyselfless = Без смены облика
end-conduct-wishless = Без желаний
end-conduct-artiwishless = Без артефактов по желанию
end-conduct-sokoban = Правила Сокобана соблюдены
end-conduct-petless = Без питомца
end-chronicle = Хроника
end-chronicle-none = Событий нет.
end-topten = Таблица рекордов
end-topten-none = Рекордов пока нет.
end-topten-place = №
end-topten-points = Очки
end-topten-hero = Герой
end-topten-hp = ОЗ

error-title = Что-то пошло не так
error-continue = Продолжить { $name }
error-to-title = В меню
error-quit = Выход

## Строка подсказки, приказы и почему они прерываются

prompt-press-key = Нажмите любую клавишу
prompt-press-key-pad = Нажмите { $a }
prompt-more = --Дальше--  (любая клавиша)
prompt-more-pad = --Дальше--  ({ $a })
prompt-saving = Сохранение игры...
prompt-pick-spot = Выберите место.
prompt-getpos = { $goal }  (. , ; : выбрать, ? справка, Esc: отмена)
prompt-getpos-pad = { $goal }  ({ $a }: выбрать, { $b }: отмена)
order-walk = В пути
order-walk-pick-up = За предметом
order-stairs-up = К лестнице вверх
order-stairs-down = К лестнице вниз
order-open-door = К двери
order-attack = В атаку
order-hold = Клавиша зажата
order-rest = Отдых, пока здоровье и энергия не восстановятся
order-line = { $what }  —  любая клавиша остановит
order-line-pad = { $what }  —  любая кнопка остановит
stop-arrived = На месте
stop-done = Готово
stop-healed = Отдых окончен: здоровье и энергия полны
stop-one-action = В бою — одно действие за раз
stop-released = Клавиша отпущена
stop-stopped = Остановлено
stop-panel = Остановлено: открыта панель
stop-focus = Остановлено: окно потеряло фокус
stop-question = Остановлено: вопрос
stop-hostile = Остановлено: враг рядом
stop-hurt = Остановлено: вы ранены
stop-hunger = Остановлено: изменился голод
stop-condition = Остановлено: изменилось ваше состояние
stop-message = Остановлено: { $message }
stop-level = Остановлено: другой уровень
stop-blocked = Остановлено: путь преграждён
stop-no-path = Путь туда неизвестен
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
hud-attrs-tip = Характеристики. Щелчок: ваш персонаж ({ $key }).
hud-xp-tip = Опыт
hud-inventory-tip = Инвентарь ({ $key })
hud-spells-tip = Заклинания ({ $key })
hud-character-tip = Персонаж ({ $key })
hud-overview-tip = Обзор подземелья ({ $key })
hud-history-tip = История сообщений ({ $key })
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
mode-explore-badge = РАЗВЕДКА
mode-explore = РАЗВЕДКА

## RPG-панели: лист персонажа (^X)

panel-hint = ↑↓ PgUp PgDn: прокрутка · Esc, { $key }: закрыть
panel-close-tip = Закрыть (Esc, { $key })
sheet-title = Персонаж
sheet-hit-points = Здоровье
sheet-energy = Энергия
sheet-armor-class = Класс брони
sheet-experience = Опыт
sheet-to-level = { $points } до { $level }-го уровня
sheet-gold = Золото
sheet-strength = Сила
sheet-dexterity = Ловкость
sheet-constitution = Телосложение
sheet-intelligence = Интеллект
sheet-wisdom = Мудрость
sheet-charisma = Харизма
sheet-base = основа { $value }
sheet-peak = пик { $value }
sheet-limit = предел { $value }
sheet-innate-limit = врождённый предел { $value }
sheet-background = Происхождение
sheet-basics = Основное
sheet-characteristics = Характеристики
sheet-status = Состояние
sheet-attributes = Свойства
sheet-miscellaneous = Разное

## RPG-панели: заклинания (+, Z)

panel-cancel-tip = Отмена (Esc)
spells-title = Заклинания
spells-cast-title = Произнести заклинание
spells-swap-title = С каким заклинанием поменять { $letter }?
spells-attack = Атака
spells-healing = Лечение
spells-divination = Прорицание
spells-enchantment = Очарование
spells-clerical = Жречество
spells-escape = Бегство
spells-matter = Материя
spells-school-level = { $school } · { $level }-й уровень
spells-fail = провал
spells-gone = забыто
spells-sort = Упорядочить заклинания
spells-reorder = Переставить
spells-cast-instead = Произнести
spells-hint-view = Буква, Enter или щелчок: сотворить · Tab: переставить · Esc, { $key }: закрыть
spells-hint-reorder = Выберите заклинание, которое переставить, затем то, с которым поменять · Tab: колдовать · Esc, { $key }: закрыть
spells-hint-cast = Буква, Enter или щелчок: сотворить · Esc, { $key }: закрыть
spells-hint-swap = Выберите заклинание, с которым поменять · Esc: оставить как есть

## RPG-панели: навыки (#enhance)

skills-title = Навыки
skills-title-slots = Навыки · свободно: { $slots }
skills-fighting = Бой
skills-weapons = Оружие
skills-spellcasting = Магия
skills-restricted = Недоступен
skills-unskilled = Без навыка
skills-basic = Базовый
skills-skilled = Опытный
skills-expert = Эксперт
skills-master = Мастер
skills-grand-master = Великий мастер
skills-can-advance = улучшить
skills-later = позже
skills-peaked = предел
skills-hint-advance = Буква, Enter или щелчок: улучшить · Esc, { $key }: закрыть

## RPG-панели: журнал (#chronicle)

journal-title = Журнал
journal-events = События
journal-major-events = Главные события
journal-messages = Сообщения
journal-no-events = Событий пока нет.
journal-no-messages = Сообщений пока нет.
journal-hint = ↑↓ PgUp PgDn: сообщения · Esc, { $key }: закрыть
journal-hint-esc = ↑↓ PgUp PgDn: сообщения · Esc: закрыть

## RPG-панели: кодекс (\, #conduct)

codex-title = Кодекс
codex-discoveries = Открытия
codex-conduct = Испытания
codex-hint = ↑↓ PgUp PgDn: прокрутка · Tab: другая вкладка · Esc, { $key }: закрыть
codex-hint-esc = ↑↓ PgUp PgDn: прокрутка · Tab: другая вкладка · Esc: закрыть

## Собственные заголовки и пояснения файлов авторов (сами указания
## авторства остаются как написаны)

credits-md-art = Графика: авторы
credits-md-art-cc0 = Всё, что перечислено в разделе «Графика: авторы», передано в общественное достояние (CC0 1.0, https://creativecommons.org/publicdomain/zero/1.0/). Указывать авторов не обязательно — мы делаем это с удовольствием.
credits-md-art-changes = Текстуры крупнее 1024 px (512 px для набора SigilsVault) уменьшены и пересжаты, анимированные текстуры переведены из TGA в PNG без уменьшения; карты ambientCG и TextureCan пересобраны (альбедо, нормали OpenGL, AO, шероховатость и металл, свечение), металлам доспехов (Chainmail004, Metal038) дано более светлое альбедо и большая шероховатость; больше ничего не изменено.
credits-md-ccby = CC-BY
credits-md-ccby-models = Модели из раздела «CC-BY» распространяются по лицензии Creative Commons Attribution 3.0 (https://creativecommons.org/licenses/by/3.0/), с сайта Poly Pizza. Они пришли статичными сетками; их сварили, масштабировали, привязали к скелетам быка или волка от Quaternius (выше, CC0) с их анимациями, а некоторым дорисовали детали — подробный список изменений на английской версии этой страницы; текстуры уменьшены.
credits-md-fonts = Шрифты
credits-md-fonts-ofl = Шрифты интерфейса распространяются по лицензии SIL Open Font License 1.1 (не CC0).
credits-md-sound = Звук: авторы
credits-md-sound-licences = Звуки из раздела «Общественное достояние» переданы в общественное достояние (CC0 1.0, https://creativecommons.org/publicdomain/zero/1.0/) или являются работами федерального правительства США, находящимися в общественном достоянии; указывать авторов не обязательно — мы делаем это с удовольствием. Звуки из раздела «Creative Commons Attribution» распространяются по лицензии Creative Commons Attribution от названных ниже авторов (CC BY 3.0, https://creativecommons.org/licenses/by/3.0/, или CC BY 4.0, https://creativecommons.org/licenses/by/4.0/, как сказано в каждой строке; OGA-BY 3.0, https://opengameart.org/content/oga-by-30-faq, — это CC BY 3.0 без пункта о технических мерах защиты).
credits-md-sound-changes = Что изменено: взятые из каждого источника файлы обрезаны (убрана тишина, дубли разделены, из длинной записи взят отрывок), переведены в моно 44,1 кГц WAV (короткие звуки) или пересжаты в Ogg Vorbis (окружение, музыка, джинглы; в моно — петли, звучащие у лавы, воды и фонтанов) и приведены к одной громкости для каждого вида; конец каждой петли плавно сведён с её началом. Фоны из раздела «Смеси» собраны из названных там источников.
credits-md-public-domain = Общественное достояние
credits-md-cc-by = Creative Commons Attribution
credits-md-mixes = Смеси
credits-md-help = Источники справки
credits-md-help-about = Справка в игре (F1 или кнопка «?» на панели) показывает руководство NetHack на языке интерфейса — по главам, с поиском.
credits-md-english = Английская
credits-md-help-english = «A Guide to the Mazes of Menace (Guidebook for NetHack)»: исходная версия — Eric S. Raymond, исправлено и дополнено для NetHack 5.0 — Mike Stephenson и другие. Часть NetHack, распространяется по лицензии NetHack General Public License.
credits-md-russian = Русская
credits-md-help-russian = «Руководство по Грозным Лабиринтам» — русский перевод руководства (к NetHack 3.4) из <https://github.com/velikodniy/nethack-guide>, коммит 576aa83 (2014-09-24). Переведено на notabenoid.com (по состоянию на 2014-02-01; переводчик witmolif@gmail.com); редактура и вёрстка — Vadim Velikodniy.
credits-md-help-licence = В репозитории перевода нет файла лицензии. Это перевод документации NetHack, и он распространяется здесь, как и само руководство, по лицензии NetHack General Public License.
credits-md-translation = Авторы русского перевода
credits-md-memory = Память переводов
credits-md-forms = Формы русских слов
credits-md-established = Устоявшиеся русские названия
credits-md-established-about = Если у существа или вещи уже есть русское название, глоссарий берёт его, а не придумывает новое; источник указан в примечании. Названия взяты из русских изданий *Dungeons & Dragons* (имена чудовищ D&D: бурый увалень, ржавник...), у Дж. Р. Р. Толкина (умертвие, призрак, урук-хай, Оркрист, Жало), у Льюиса Кэрролла в переводе Д. Орловской (бармаглот, Вострый Клинок), у Терри Пратчетта (Двацветок, Анк-Морпорк, Слепой Ио, Оффлер, Госпожа), у Фрица Лейбера (Иссек, Мог, Кос), у М. Муркока (Буревестник), а также из русской мифологии, истории и естествознания (левкрота, сулица, бердыш).

## Журнал сообщений

log-title = Сообщения
log-history-title = История сообщений
log-history-close = Закрыть  { $key }
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
dlg-yes-pad = Да
dlg-no-pad = Нет
dlg-cancel-q-pad = Отмена
dlg-all-a-pad = Все
dlg-ok = ОК
dlg-cancel = Отмена
dlg-count = Количество: { $n } — выберите предмет, чтобы взять столько
dlg-selected = Выбрано: { $n }
dlg-hint-read = ↑↓ PgUp PgDn < >: прокрутка · Enter или Esc: закрыть
dlg-space-toggle = Пробел: отметить её
dlg-space-then = затем Пробел отметит её
dlg-hint-any = буква: отметить · ↑↓ выбрать строку, { $space } · '.' все · '-' ни одной · '@' обратить выбор · цифры: количество · PgUp PgDn < >: прокрутка · Enter: ОК · Esc: отмена
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
# слова, по которым палитра `#` находит расширенную команду, кроме её
# имени и описания, через запятую, иначе само имя (имена команд остаются
# английскими: их и набирают)
palette-also = { $cmd ->
        [adjust] буквы, переназначить
        [annotate] заметка, название уровня
        [apply] использовать, применить
        [attributes] характеристики, атрибуты
        [autopickup] автоподбор
        [call] назвать, имя
        [cast] заклинание, колдовать, магия
        [chat] разговор, говорить, беседа
        [chronicle] летопись, хроника, дневник
        [close] закрыть дверь
        [conduct] обеты, испытания, поведение
        [dip] макнуть, окунуть, обмакнуть
        [down] вниз, спуститься, лестница
        [drop] положить, выложить, оставить
        [droptype] положить, выложить, оставить
        [eat] еда, есть, съесть
        [engrave] надпись, гравировка, писать, элберет
        [enhance] навыки, умения, улучшить
        [fight] атака, драться, бой
        [fire] стрелять, выстрел, стрельба
        [force] взлом, сломать замок
        [genocided] геноцид, истреблённые
        [help] справка, помощь
        [history] история
        [inventory] инвентарь, вещи
        [inventtype] инвентарь, вещи
        [invoke] призвать, задействовать, артефакт
        [jump] прыжок, прыгать
        [kick] пинок, пнуть, удар ногой
        [known] открытия, известные
        [knownclass] открытия, известные
        [look] осмотреть, смотреть
        [lookaround] осмотреться, оглядеться
        [loot] добыча, ящик, сундук
        [monster] способность
        [name] назвать, имя
        [offer] жертва, жертвоприношение, алтарь
        [open] открыть дверь
        [options] параметры, настройки
        [optionsfull] параметры, настройки
        [overview] обзор, подземелье
        [pay] платить, оплата, лавка
        [pickup] поднять, подобрать, взять
        [pray] молитва, молиться
        [prevmsg] сообщения, журнал
        [puton] надеть, кольцо, амулет
        [quaff] пить, выпить, зелье
        [quit] сдаться, give up
        [quiver] колчан
        [read] читать, прочесть
        [remove] снять, кольцо, амулет
        [repeat] повтор, повторить
        [retravel] путешествие, идти
        [ride] езда, верхом, скакун
        [rub] тереть
        [save] сохранить, сохранение
        [search] поиск, искать, обыскать
        [seeall] снаряжение
        [showgold] золото, деньги
        [showspells] заклинания
        [showtrap] ловушка
        [sit] сидеть
        [swap] сменить оружие
        [takeoff] снять доспех
        [takeoffall] снять всё
        [teleport] телепорт
        [terrain] местность, рельеф
        [throw] метнуть, бросить
        [tip] высыпать, вытряхнуть
        [travel] путешествие, идти
        [turn] изгнать, нежить
        [twoweapon] два оружия
        [untrap] обезвредить, ловушка
        [up] вверх, подняться, лестница
        [vanquished] убитые, побеждённые
        [version] версия
        [wait] ждать, отдых
        [wear] надеть, доспех
        [whatdoes] что делает
        [whatis] что это
        [wield] вооружиться, оружие
        [wipe] вытереть
        [zap] палочка, жезл
       *[other] { $cmd }
    }
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
item-throw = Метнуть
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
item-drop = Положить
item-drop-some = Положить часть…
item-adjust = Сменить букву…
item-split = Разделить стопку…
item-name = Назвать предмет…
item-call = Назвать вид…
item-dip = Окунуть в…
item-two-weapon = Бой с двух рук
item-force = Взломать замок
item-rub = Потереть о…
item-tip = Вытряхнуть
inv-all = Все
inv-suggested = Подходящие
inv-weapons = Оружие
inv-armor = Доспехи
inv-accessories = Кольца и амулеты
inv-tools = Инструменты
inv-food = Еда
inv-potions = Зелья
inv-scrolls-and-books = Свитки и книги
inv-wands = Палочки
inv-gems-and-other = Камни и прочее
inv-equipped = Надето
cmd-search = Искать
cmd-rest = Отдыхать до исцеления
cmd-wait = Ждать
cmd-kick = Пнуть
cmd-pick-up = Поднять
cmd-look-here = Что здесь лежит
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
cmd-two-weapon = Бой с двух рук
cmd-enhance = Развить навыки
cmd-terrain = Местность
cmd-overview = Обзор
cmd-attributes = Характеристики
cmd-discoveries = Открытия
cmd-cast = Произнести заклинание
cmd-throw = Метнуть
cmd-engrave = Писать на полу
cmd-up = Подняться
cmd-down = Спуститься

## Панель инвентаря

class-weapon = Оружие
class-armor = Доспех
class-ring = Кольцо
class-amulet = Амулет
class-tool = Инструмент
class-comestible = Еда
class-potion = Зелье
class-scroll = Свиток
class-spellbook = Книга заклинаний
class-wand = Палочка
class-gem = Камень
class-boulder = Валун или статуя
class-iron-ball = Железное ядро
class-iron-chain = Железная цепь
class-venom = Яд
class-coins = Монеты
class-item = Предмет
fact-blessed =
    { $form ->
        [f] Благословлена
        [n] Благословлено
        [pl] Благословлены
       *[m] Благословлён
    }
fact-uncursed =
    { $form ->
        [f] Не проклята
        [n] Не проклято
        [pl] Не прокляты
       *[m] Не проклят
    }
fact-cursed =
    { $form ->
        [f] Проклята
        [n] Проклято
        [pl] Прокляты
       *[m] Проклят
    }
fact-enchantment = Зачарование { $value }
fact-containing = Внутри: { $what }
fact-containing-items =
    { $n ->
        [one] Внутри { $n } предмет
        [few] Внутри { $n } предмета
       *[many] Внутри { $n } предметов
    }
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
inv-confirm-pad = Подтвердить  { $start }
inv-cancel-pad = Отмена  { $b }
inv-close-tip = Закрыть (Esc или i)
inv-suggested-tip = Подходящие (?)
inv-search = Поиск
inv-pack-order = Порядок вещей, как в NetHack
inv-detail-empty = Выберите предмет, чтобы узнать, что вам о нём известно.
inv-count-hint = Введите число или используйте ← →.  Enter: ОК · Esc: отмена
inv-count-hint-pad = ← →: количество · { $a }: ОК · { $b }: отмена
inv-count = { $verb }?  { $value } / { $most }
inv-count-how-many = Сколько
inv-count-drop = Сколько положить
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
    '.' все · '-' ни одного · '@' обратить выбор · Enter: подтвердить · Esc: отмена
inv-menu-one-hint = Щёлкните или нажмите букву, чтобы выбрать · Esc: отмена
inv-carrying = Предмет в руках
inv-carrying-hint = Наведите туда, куда положить (ячейка куклы, другой предмет), и снова нажмите Y · B: вернуть на место
inv-browse-hint =
    Перетащите на куклу — надеть · на панель действий — назначить · за панель — положить на пол
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
    Перезарядок: { $times } · { $charges ->
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
hint-menu = Меню
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
hint-change = Изменить
hint-section = Раздел
hint-answer = Ответить
hint-run = Выполнить
radial-title = Действия
radial-hint = Наведите стик на действие · отпустите в центре: ничего
radial-let-go = Отпустите { $lt }, чтобы выполнить
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

## Выбор названий: желание, монстр, класс (ввод по-русски)

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
picker-class-wand = Палочки
picker-class-coin = Монеты
picker-class-gem = Самоцветы и камни
picker-class-heavy = Валуны, статуи, железо
picker-placeholder-wish = Например: благословенный +2 длинный меч
picker-placeholder-monster = Название монстра
picker-placeholder-class = Класс или один из его монстров
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
achievement-rank-4-name = Закалённый в боях
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
achievement-ascend-illiterate-name = Вознесение без чтения
achievement-ascend-illiterate-desc = Вознеситесь, ничего не прочитав.
achievement-ascend-polypileless-name = Вознесение без превращений вещей
achievement-ascend-polypileless-desc = Вознеситесь, не превратив ни одного предмета.
achievement-ascend-polyselfless-name = Вознесение в своём облике
achievement-ascend-polyselfless-desc = Вознеситесь, ни разу не сменив облик.
achievement-ascend-wishless-name = Вознесение без желаний
achievement-ascend-wishless-desc = Вознеситесь, не загадав ни одного желания.
achievement-ascend-artiwishless-name = Вознесение без загаданных артефактов
achievement-ascend-artiwishless-desc = Вознеситесь, не пожелав ни одного артефакта.
achievement-ascend-petless-name = Вознесение без питомца
achievement-ascend-petless-desc = Вознеситесь, ни разу не заведя питомца.
achievement-sokoban-purist-name = Честный Сокобан
achievement-sokoban-purist-desc = Заберите награду Сокобана, ни разу не нарушив его правил.

## Справка: руководство по NetHack

hud-help-tip = Справка: путеводитель и руководство ({ $key })
help-title = Справка — { $book }
help-close-tip = Закрыть (Esc, { $key })
help-search-placeholder = Поиск по руководству
help-nothing = Ничего не найдено
help-hint = ↑↓: глава · PgUp PgDn: прокрутка · Esc: закрыть
help-chord = [b]Нет цифровой клавиатуры?[/b] Нажмите две стрелки вместе, чтобы шагнуть по диагонали: ↑ и ← — на северо-запад, ↑ и → — на северо-восток, ↓ и ← — на юго-запад, ↓ и → — на юго-восток. Если держать их, герой идёт дальше; с Shift — бежит. Так же стрелки отвечают на вопрос о направлении и двигают курсор по карте.
guide-card-kicker = Подсказка
guide-card-close = Esc · B · щелчок: закрыть
guide-quest-title = Ваш поход
guide-quest = Глубоко внизу лежит Амулет Йендора. Спуститесь по Подземельям Рока, найдите его, вынесите наверх и принесите в дар своему богу.
guide-pray-title = В беде? Молитесь
guide-pray = Слабость от голода или мало здоровья: помолитесь (#pray). Бог может помочь, если только вы не молились недавно: между молитвами пусть пройдёт около тысячи ходов.
guide-pray-pad = Слабость от голода или мало здоровья: помолитесь (зажмите { $lt }, затем «Молиться»). Бог может помочь, если только вы не молились недавно: между молитвами пусть пройдёт около тысячи ходов.
guide-monster-title = Существо
guide-monster = Идите на него, чтобы сразиться, или щёлкните по нему. Если существо мирное, игра переспросит; { $fight } и направление бьют, не сходя с места.
guide-monster-pad = Идите на него, чтобы сразиться. Если существо мирное, игра переспросит. Бить, не сходя с места: зажмите { $lt }, выберите «Атаковать», затем направление.
guide-wounded-title = Ранение
guide-wounded = Здоровье понемногу возвращается само. Отдыхайте ({ $rest }) или ищите тайники ({ $search }), когда рядом никого нет.
guide-wounded-pad = Здоровье понемногу возвращается само. Отдыхайте (зажмите { $lt }, затем «Отдых») или ищите тайники ({ $x }), когда рядом никого нет.
guide-hungry-title = Голод
guide-hungry = Поешьте поскорее ({ $eat } или «Съесть» в меню предмета), иначе ослабеете.
guide-hungry-pad = Поешьте поскорее («Съесть» в меню предмета: { $y } открывает инвентарь), иначе ослабеете.
guide-shop-title = Лавка
guide-shop = Поднимите вещь, чтобы узнать цену, и заплатите ({ $pay }). Уйдёте с неоплаченным товаром — лавочник нападёт.
guide-shop-pad = Поднимите вещь, чтобы узнать цену, и заплатите: меню игры ({ $start }), «Все команды», затем pay. Уйдёте с неоплаченным товаром — лавочник нападёт.
guide-item-here-title = Здесь что-то лежит
guide-item-here = Поднимите ({ $pickup }) или щёлкните по герою.
guide-item-here-pad = Поднимите: зажмите { $lt }, затем «Поднять».
guide-unidentified-title = Неизвестная вещь
guide-unidentified = Вы пока не знаете, что это: название откроется, когда вы примените вещь или опознаете её.
guide-level-up-title = Новый уровень
guide-level-up = Больше здоровья и точнее удары. Ваше звание — под именем.
guide-enhance-title = Навык можно улучшить
guide-enhance = #enhance открывает навыки; те, что готовы к улучшению, отмечены.
guide-enhance-pad = Навыки — в меню игры ({ $start }); те, что готовы к улучшению, отмечены.
guide-pet-title = Ваш питомец
guide-pet = Он сражается рядом с вами. Держите его рядом на лестнице, чтобы взять с собой. На проклятые вещи он ступает неохотно.
guide-trap-title = Ловушка
guide-trap = Обойдите её. Если ловушка известна, игра переспросит, прежде чем вы на неё ступите.
guide-stairs-down-title = Лестница вниз
guide-stairs-down = Встаньте на неё и нажмите { $down } или щёлкните по ней, чтобы спуститься глубже. Амулет лежит далеко внизу.
guide-stairs-down-pad = Встаньте на неё, нажмите { $a } и выберите спуск; или наведите на неё курсор правым стиком и нажмите { $a }. Амулет лежит далеко внизу.
guide-door-title = Дверь
guide-door = Шагните в закрытую дверь, чтобы открыть её. Запертую выбейте ({ $kick }) или отоприте ключом.
guide-door-pad = Шагните в закрытую дверь, чтобы открыть её. Запертую выбейте (зажмите { $lt }, затем «Пнуть») или отоприте ключом.
guide-fountain-title = Фонтан
guide-fountain = Пить из него ({ $quaff }) или окунать в него вещи — азартная игра: удача выпадает редко.
guide-fountain-pad = Пить из него (встаньте на него и нажмите { $a }) или окунать в него вещи — азартная игра: удача выпадает редко.
guide-altar-title = Алтарь
guide-altar = Положите на него вещи, чтобы узнать, благословенны они или прокляты.
help-guide = Путеводитель
help-guide-about = Путеводитель этой игры: «Ваш поход» об экране, передвижении и бое, затем карточки, которые вы уже видели, чтобы перечитать их (Настройки → Интерфейс → Подсказки включают и выключают карточки). Дальше идёт руководство самого NetHack; главы с пометкой «терминал» описывают экран терминальной версии NetHack.
# пометка после глав руководства об экране в списке справки
help-terminal-tag = · терминал
# первые слова этих глав; $page: первая страница путеводителя
help-terminal-note = Эта глава описывает экран терминальной версии NetHack: карту из букв, строки состояния внизу, сообщения вверху. В этой игре экран другой: о нём рассказывает «{ $page }», первая страница справки.
quest-goal-more = Путь долог, и подземелье в каждой игре своё. Эти страницы покажут, что к чему; путеводитель в справке хранит их и карточки, что появятся по ходу игры.
quest-screen-title = Ваш экран
quest-screen = По краям экрана игра показывает то, что здесь подписано.
# $banner: mode-combat, $badge: mode-combat-badge
quest-screen-combat = В бою появляется надпись { $banner }, а режим сменяется на { $badge }: с этого момента каждое действие ждёт вас.
quest-screen-overview = { $overview }: весь уровень, каким вы его знаете; ещё раз — снова к герою.
quest-screen-overview-pad = { $l3 } (нажатый левый стик): весь уровень, каким вы его знаете; ещё раз — снова к герою.
quest-screen-tips = Наведите мышь на любую из этих частей, чтобы прочесть подсказку.
quest-tour-portrait = Вы: звание, мировоззрение и характеристики
quest-tour-hp = Здоровье: кончится — вы умрёте
quest-tour-pw = Энергия для заклинаний
quest-tour-minimap = Уровень, каким вы его знаете
quest-tour-mode = Режим: разведка или бой
quest-tour-buttons = Панели: вещи, заклинания, герой, подземелье…
quest-tour-buttons-pad = Панели, все они и в меню { $start }
quest-tour-bar = Действия: { $first }…{ $last } или щелчок
quest-tour-bar-pad = Действия: { $lb }, { $rb } или оба вместе с { $a } { $b } { $x } { $y }
quest-tour-log = Что происходит, словами
quest-moves-title = Движение и бой
quest-moves-keys = Ходите: { $steps }. Нет цифровой клавиатуры? Нажмите две стрелки вместе, чтобы шагнуть по диагонали.
quest-steps-modern = стрелки, Home, PgUp, End и PgDn или цифровая клавиатура
quest-steps-classic = h j k l y u b n или стрелки
quest-moves-fight = Идите на существо, чтобы сразиться с ним. { $fight } и направление бьют, не сходя с места.
quest-moves-click = Щелчок по месту ведёт туда, по существу — в бой, по вещи — поднять её; правый щелчок — что можно сделать там.
quest-moves-pad = С геймпадом: левый стик ведёт героя, { $a } действует, зажатый { $lt } открывает круг действий.
quest-moves-pad-walk = Левый стик или крестовина ведут героя; если держать, герой идёт дальше.
quest-moves-pad-fight = Идите на существо, чтобы сразиться. { $rt } — выстрел; зажатый { $lt } открывает круг действий: поднять, бить на месте, пнуть, отдых, молитва, путь.
quest-moves-pad-cursor = Правый стик водит курсор по карте; { $a } делает то, что там можно: идти, сражаться, поднять, а на герое — действия здесь.
quest-moves-pad-more = { $y }: инвентарь · { $x }: искать тайники · { $back }: сообщения · { $start }: меню игры, в нём все панели и команды
quest-moves-more = { $inventory }: инвентарь · { $palette }: все команды · { $help }: справка
quest-next = Далее
quest-back = Назад
quest-skip = Пропустить
quest-play = Играть
quest-hint = Любая клавиша или щелчок: далее · ←: назад · Esc: пропустить

## Меню игры (Esc в ожидании команды)

game-menu-title = Игра
game-menu-resume = Вернуться в игру
game-menu-settings = Настройки
game-menu-save = Сохранить и выйти
game-menu-give-up = Сдаться
game-menu-give-up-about = Сдаться — значит закончить эту игру навсегда: герой погибнет, ничего не сохранится.
game-menu-help = Справка
game-menu-commands = Все команды
game-menu-hint = ↑↓ ←→: выбрать · Enter: выполнить · Esc: вернуться в игру
quit-warning = Герой погибнет здесь, и игра закончится навсегда: она не сохранится, и продолжить её будет нельзя.
palette-give-up = Сдаться: закончить эту игру навсегда, без сохранения
hint-chapter = Глава
hint-scroll = Прокрутка

## Состояние: уровень подземелья, опыт и очки

hud-dlvl = Глубина { $level }
hud-tutorial-level = Обучение { $level }
# the Quest's levels, as the status line says them
hud-home-level = Дом { $level }
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
genocided-extinct = вымерли
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
overview-quest-done = Квест для { $leader } выполнен
overview-quest-given = Квест получен от { $leader }
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
overview-one-way-up = Лестница без возврата, вверх в { $place }
overview-one-way-down = Лестница без возврата, вниз в { $place }
overview-portal = Портал в { $place }
overview-sealed-portal = Закрытый портал в { $place }
overview-connection = Проход в { $place }
overview-unknown-way = Путь в { $place }
overview-branch-level = { $branch }, уровень { $level }
overview-resting = Здесь покоятся
overview-resting-you = Здесь покоитесь вы
overview-dead-you = { $how }
overview-dead = { $who }, { $how }

# the prologue: NetHack's legacy told over scenes before a game
prologue-hint = Любая клавиша или A — дальше  ·  Esc или B — пропустить
prologue-hint-pad = { $a } — дальше  ·  { $b } — пропустить
title-prologue = Пролог
settings-prologue = Пролог
settings-prologue-about = История Амулета, как её рассказывает NetHack, — в сценах перед игрой.
settings-prologue-first = Перед первой игрой
settings-prologue-every = Перед каждой новой игрой
settings-prologue-never = Никогда
