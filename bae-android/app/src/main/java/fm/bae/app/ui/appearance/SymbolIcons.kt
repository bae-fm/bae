package fm.bae.app.ui.appearance

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.graphics.vector.group
import androidx.compose.ui.unit.dp

/**
 * The icons the app draws, in the shapes of Google's Material Symbols Rounded at weight 400 and optical
 * size 24, which the design system's boards draw them in. Play, pause, previous and next are the filled
 * symbols; every other icon is the outline. The icons that point in a reading direction mirror in a
 * right-to-left layout.
 */
internal object SymbolIcons {
    val Close: ImageVector by lazy {
        symbol(
            "Close",
            mirrored = false,
            pathData =
                """
            M480-424 284-228q-11 11-28 11t-28-11q-11-11-11-28t11-28l196-196-196-196q-11-11-11-28t11-28q11-11
            28-11t28 11l196 196 196-196q11-11 28-11t28 11q11 11 11 28t-11 28L536-480l196 196q11 11 11 28t-11
            28q-11 11-28 11t-28-11L480-424Z
                """,
        )
    }

    val ArrowBack: ImageVector by lazy {
        symbol(
            "ArrowBack",
            mirrored = true,
            pathData =
                """
            m313-440 196 196q12 12 11.5 28T508-188q-12 11-28 11.5T452-188L188-452q-6-6-8.5-13t-2.5-15q0-8
            2.5-15t8.5-13l264-264q11-11 27.5-11t28.5 11q12 12 12 28.5T508-715L313-520h447q17 0 28.5
            11.5T800-480q0 17-11.5 28.5T760-440H313Z
                """,
        )
    }

    val MoreVert: ImageVector by lazy {
        symbol(
            "MoreVert",
            mirrored = false,
            pathData =
                """
            M480-160q-33 0-56.5-23.5T400-240q0-33 23.5-56.5T480-320q33 0 56.5 23.5T560-240q0 33-23.5
            56.5T480-160Zm0-240q-33 0-56.5-23.5T400-480q0-33 23.5-56.5T480-560q33 0 56.5 23.5T560-480q0
            33-23.5 56.5T480-400Zm0-240q-33 0-56.5-23.5T400-720q0-33 23.5-56.5T480-800q33 0 56.5
            23.5T560-720q0 33-23.5 56.5T480-640Z
                """,
        )
    }

    /** The heart a rating shows, filled: the rating systems' Heart. */
    val Favorite: ImageVector by lazy {
        symbol(
            "Favorite",
            mirrored = false,
            pathData =
                """
            m480-120-58-52q-101-91-167-157T150-447.5Q111-500 95.5-544T80-634q0-94 63-157t157-63q52 0 99 22t81
            62q34-40 81-62t99-22q94 0 157 63t63 157q0 46-15.5 90T810-447.5Q771-395 705-329T538-172l-58 52Z
                """,
        )
    }

    /** The heart's outline, for a rating that is not a heart. */
    val FavoriteBorder: ImageVector by lazy {
        symbol(
            "FavoriteBorder",
            mirrored = false,
            pathData =
                """
            m480-120-58-52q-101-91-167-157T150-447.5Q111-500 95.5-544T80-634q0-94 63-157t157-63q52 0 99 22t81
            62q34-40 81-62t99-22q94 0 157 63t63 157q0 46-15.5 90T810-447.5Q771-395 705-329T538-172l-58
            52Zm0-108q96-86 158-147.5t98-107q36-45.5 50-81t14-70.5q0-60-40-100t-100-40q-47 0-87 26.5T518-680h-76q-15-41-55-67.5T300-774q-60 0-100
            40t-40 100q0 35 14 70.5t50 81q36 45.5 98 107T480-228Zm0-273Z
                """,
        )
    }

    val Add: ImageVector by lazy {
        symbol(
            "Add",
            mirrored = false,
            pathData =
                """
            M440-440H240q-17 0-28.5-11.5T200-480q0-17 11.5-28.5T240-520h200v-200q0-17 11.5-28.5T480-760q17 0
            28.5 11.5T520-720v200h200q17 0 28.5 11.5T760-480q0 17-11.5 28.5T720-440H520v200q0 17-11.5
            28.5T480-200q-17 0-28.5-11.5T440-240v-200Z
                """,
        )
    }

    val PlayArrow: ImageVector by lazy {
        symbol(
            "PlayArrow",
            mirrored = false,
            pathData =
                """
            M320-273v-414q0-17 12-28.5t28-11.5q5 0 10.5 1.5T381-721l326 207q9 6 13.5 15t4.5 19q0 10-4.5
            19T707-446L381-239q-5 3-10.5 4.5T360-233q-16 0-28-11.5T320-273Z
                """,
        )
    }

    val Pause: ImageVector by lazy {
        symbol(
            "Pause",
            mirrored = false,
            pathData =
                """
            M640-200q-33 0-56.5-23.5T560-280v-400q0-33 23.5-56.5T640-760q33 0 56.5 23.5T720-680v400q0
            33-23.5 56.5T640-200Zm-320 0q-33 0-56.5-23.5T240-280v-400q0-33 23.5-56.5T320-760q33 0 56.5
            23.5T400-680v400q0 33-23.5 56.5T320-200Z
                """,
        )
    }

    val SkipPrevious: ImageVector by lazy {
        symbol(
            "SkipPrevious",
            mirrored = false,
            pathData =
                """
            M220-280v-400q0-17 11.5-28.5T260-720q17 0 28.5 11.5T300-680v400q0 17-11.5 28.5T260-240q-17
            0-28.5-11.5T220-280Zm458-1L430-447q-9-6-13.5-14.5T412-480q0-10 4.5-18.5T430-513l248-166q5-4
            11-5t11-1q16 0 28 11t12 29v330q0 18-12 29t-28 11q-5 0-11-1t-11-5Z
                """,
        )
    }

    val SkipNext: ImageVector by lazy {
        symbol(
            "SkipNext",
            mirrored = false,
            pathData =
                """
            M660-280v-400q0-17 11.5-28.5T700-720q17 0 28.5 11.5T740-680v400q0 17-11.5 28.5T700-240q-17
            0-28.5-11.5T660-280Zm-440-35v-330q0-18 12-29t28-11q5 0 11 1t11 5l248 166q9 6 13.5 14.5T548-480q0
            10-4.5 18.5T530-447L282-281q-5 4-11 5t-11 1q-16 0-28-11t-12-29Z
                """,
        )
    }

    val Search: ImageVector by lazy {
        symbol(
            "Search",
            mirrored = false,
            pathData =
                """
            M380-320q-109 0-184.5-75.5T120-580q0-109 75.5-184.5T380-840q109 0 184.5 75.5T640-580q0 44-14
            83t-38 69l224 224q11 11 11 28t-11 28q-11 11-28 11t-28-11L532-372q-30 24-69 38t-83 14Zm0-80q75 0
            127.5-52.5T560-580q0-75-52.5-127.5T380-760q-75 0-127.5 52.5T200-580q0 75 52.5 127.5T380-400Z
                """,
        )
    }

    val RepeatOne: ImageVector by lazy {
        symbol(
            "RepeatOne",
            mirrored = false,
            pathData =
                """
            M460-540h-30q-13 0-21.5-8.5T400-570q0-13 8.5-21.5T430-600h50q17 0 28.5 11.5T520-560v170q0 13-8.5
            21.5T490-360q-13 0-21.5-8.5T460-390v-150ZM274-200l34 34q12 12 11.5 28T308-110q-12 12-28.5
            12.5T251-109L148-212q-6-6-8.5-13t-2.5-15q0-8 2.5-15t8.5-13l103-103q12-12 28.5-11.5T308-370q11 12
            11.5 28T308-314l-34 34h406v-120q0-17 11.5-28.5T720-440q17 0 28.5 11.5T760-400v120q0 33-23.5
            56.5T680-200H274Zm412-480H280v120q0 17-11.5 28.5T240-520q-17 0-28.5-11.5T200-560v-120q0-33
            23.5-56.5T280-760h406l-34-34q-12-12-11.5-28t11.5-28q12-12 28.5-12.5T709-851l103 103q6 6 8.5
            13t2.5 15q0 8-2.5 15t-8.5 13L709-589q-12 12-28.5 11.5T652-590q-11-12-11.5-28t11.5-28l34-34Z
                """,
        )
    }

    val Repeat: ImageVector by lazy {
        symbol(
            "Repeat",
            mirrored = false,
            pathData =
                """
            m274-200 34 34q12 12 11.5 28T308-110q-12 12-28.5 12.5T251-109L148-212q-6-6-8.5-13t-2.5-15q0-8
            2.5-15t8.5-13l103-103q12-12 28.5-11.5T308-370q11 12 11.5 28T308-314l-34 34h406v-120q0-17
            11.5-28.5T720-440q17 0 28.5 11.5T760-400v120q0 33-23.5 56.5T680-200H274Zm412-480H280v120q0
            17-11.5 28.5T240-520q-17 0-28.5-11.5T200-560v-120q0-33
            23.5-56.5T280-760h406l-34-34q-12-12-11.5-28t11.5-28q12-12 28.5-12.5T709-851l103 103q6 6 8.5
            13t2.5 15q0 8-2.5 15t-8.5 13L709-589q-12 12-28.5 11.5T652-590q-11-12-11.5-28t11.5-28l34-34Z
                """,
        )
    }

    val Shuffle: ImageVector by lazy {
        symbol(
            "Shuffle",
            mirrored = false,
            pathData =
                """
            M600-160q-17 0-28.5-11.5T560-200q0-17 11.5-28.5T600-240h64l-99-99q-12-12-11.5-28.5T566-396q12-12
            28.5-12t28.5 12l97 98v-62q0-17 11.5-28.5T760-400q17 0 28.5 11.5T800-360v160q0 17-11.5
            28.5T760-160H600Zm-428-12q-11-11-11-28t11-28l492-492h-64q-17 0-28.5-11.5T560-760q0-17
            11.5-28.5T600-800h160q17 0 28.5 11.5T800-760v160q0 17-11.5 28.5T760-560q-17
            0-28.5-11.5T720-600v-64L228-172q-11 11-28 11t-28-11Zm-1-560q-11-11-11-28t11-28q11-11
            27.5-11t28.5 11l168 167q11 11 11.5 27.5T395-565q-11 11-28 11t-28-11L171-732Z
                """,
        )
    }

    val KeyboardArrowDown: ImageVector by lazy {
        symbol(
            "KeyboardArrowDown",
            mirrored = false,
            pathData =
                """
            M480-361q-8 0-15-2.5t-13-8.5L268-556q-11-11-11-28t11-28q11-11 28-11t28 11l156 156 156-156q11-11
            28-11t28 11q11 11 11 28t-11 28L508-372q-6 6-13 8.5t-15 2.5Z
                """,
        )
    }

    val ExpandMore: ImageVector by lazy {
        symbol(
            "ExpandMore",
            mirrored = false,
            pathData =
                """
            M480-362q-8 0-15-2.5t-13-8.5L268-557q-11-11-11-28t11-28q11-11 28-11t28 11l156 156 156-156q11-11
            28-11t28 11q11 11 11 28t-11 28L508-373q-6 6-13 8.5t-15 2.5Z
                """,
        )
    }

    val ExpandLess: ImageVector by lazy {
        symbol(
            "ExpandLess",
            mirrored = false,
            pathData =
                """
            M480-529 324-373q-11 11-28 11t-28-11q-11-11-11-28t11-28l184-184q6-6 13-8.5t15-2.5q8 0 15 2.5t13
            8.5l184 184q11 11 11 28t-11 28q-11 11-28 11t-28-11L480-529Z
                """,
        )
    }

    val ChevronRight: ImageVector by lazy {
        symbol(
            "ChevronRight",
            mirrored = true,
            pathData =
                """
            M504-480 348-636q-11-11-11-28t11-28q11-11 28-11t28 11l184 184q6 6 8.5 13t2.5 15q0 8-2.5 15t-8.5
            13L404-268q-11 11-28 11t-28-11q-11-11-11-28t11-28l156-156Z
                """,
        )
    }

    val Cast: ImageVector by lazy {
        symbol(
            "Cast",
            mirrored = false,
            pathData =
                """
            M480-480Zm400-240v480q0 33-23.5 56.5T800-160H638q-17 0-28.5-11.5T598-200q0-17
            11.5-28.5T638-240h162v-480H160v6q0 17-11.5 28.5T120-674q-17 0-28.5-11.5T80-714v-6q0-33
            23.5-56.5T160-800h640q33 0 56.5 23.5T880-720ZM320-160q-16
            0-28-9.5T277-195q-11-63-56.5-107T112-357q-15-2-23.5-14.5T80-400q0-17 11-28.5t26-9.5q94 12 160.5
            79T357-198q2 16-9 27t-28 11Zm160 0q-17
            0-28.5-11T438-199q-14-128-104.5-217T115-518q-16-2-25.5-14T80-560q0-17 10.5-29t25.5-10q161 13 274
            125t128 273q2 17-9.5 29T480-160Zm-350 0q-21 0-35.5-14.5T80-210q0-21 14.5-35.5T130-260q21 0 35.5
            14.5T180-210q0 21-14.5 35.5T130-160Z
                """,
        )
    }

    val VolumeUp: ImageVector by lazy {
        symbol(
            "VolumeUp",
            mirrored = true,
            pathData =
                """
            M760-481q0-83-44-151.5T598-735q-15-7-22-21.5t-2-29.5q6-16 21.5-23t31.5 0q97 43 155
            131.5T840-481q0 108-58 196.5T627-153q-16 7-31.5 0T574-176q-5-15 2-29.5t22-21.5q74-34
            118-102.5T760-481ZM280-360H160q-17 0-28.5-11.5T120-400v-160q0-17
            11.5-28.5T160-600h120l132-132q19-19 43.5-8.5T480-703v446q0 27-24.5
            37.5T412-228L280-360Zm380-120q0 42-19 79.5T591-339q-10 6-20.5.5T560-356v-250q0-12
            10.5-17.5t20.5.5q31 25 50 63t19 80ZM400-606l-86 86H200v80h114l86 86v-252ZM300-480Z
                """,
        )
    }

    val VolumeOff: ImageVector by lazy {
        symbol(
            "VolumeOff",
            mirrored = true,
            pathData =
                """
            M671-177q-11 7-22 13t-23 11q-15 7-30.5 0T574-176q-6-15 1.5-29.5T598-227q7-3
            13-6.5t12-7.5L480-368v111q0 27-24.5 37.5T412-228L280-360H160q-17 0-28.5-11.5T120-400v-160q0-17
            11.5-28.5T160-600h88L84-764q-11-11-11-28t11-28q11-11 28-11t28 11l680 680q11 11 11 28t-11 28q-11
            11-28 11t-28-11l-93-93Zm89-304q0-83-44-151.5T598-735q-15-7-22-21.5t-2-29.5q6-16 21.5-23t31.5
            0q97 43 155 131t58 197q0 33-6 65.5T817-353q-8 22-24.5 27.5t-30.5.5q-14-5-22.5-18t-.5-30q11-26
            16-52.5t5-55.5ZM591-623q33 21 51 63t18 80v10q0 5-1 10-2 13-14
            17t-22-6l-51-51q-6-6-9-13.5t-3-15.5v-77q0-12
            10.5-17.5t20.5.5Zm-201-59q-6-6-6-14t6-14l22-22q19-19 43.5-8.5T480-703v63q0 14-12
            19t-22-5l-56-56Zm10 328v-94l-72-72H200v80h114l86 86Zm-36-130Z
                """,
        )
    }

    val Tv: ImageVector by lazy {
        symbol(
            "Tv",
            mirrored = false,
            pathData =
                """
            M160-200q-33 0-56.5-23.5T80-280v-480q0-33 23.5-56.5T160-840h640q33 0 56.5 23.5T880-760v480q0
            33-23.5 56.5T800-200H640v40q0 17-11.5 28.5T600-120H360q-17
            0-28.5-11.5T320-160v-40H160Zm0-80h640v-480H160v480Zm0 0v-480 480Z
                """,
        )
    }

    val Speaker: ImageVector by lazy {
        symbol(
            "Speaker",
            mirrored = false,
            pathData =
                """
            M680-80H280q-33 0-56.5-23.5T200-160v-640q0-33 23.5-56.5T280-880h400q33 0 56.5 23.5T760-800v640q0
            33-23.5 56.5T680-80Zm0-80v-640H280v640h400ZM480-600q33 0
            56.5-23.5T560-680q0-33-23.5-56.5T480-760q-33 0-56.5 23.5T400-680q0 33 23.5 56.5T480-600Zm0
            400q66 0 113-47t47-113q0-66-47-113t-113-47q-66 0-113 47t-47 113q0 66 47 113t113 47Zm0-80q-33
            0-56.5-23.5T400-360q0-33 23.5-56.5T480-440q33 0 56.5 23.5T560-360q0 33-23.5
            56.5T480-280ZM280-800v640-640Z
                """,
        )
    }

    val Info: ImageVector by lazy {
        symbol(
            "Info",
            mirrored = false,
            pathData =
                """
            M480-280q17 0 28.5-11.5T520-320v-160q0-17-11.5-28.5T480-520q-17 0-28.5 11.5T440-480v160q0 17
            11.5 28.5T480-280Zm0-320q17 0 28.5-11.5T520-640q0-17-11.5-28.5T480-680q-17 0-28.5 11.5T440-640q0
            17 11.5 28.5T480-600Zm0 520q-83 0-156-31.5T197-197q-54-54-85.5-127T80-480q0-83
            31.5-156T197-763q54-54 127-85.5T480-880q83 0 156 31.5T763-763q54 54 85.5 127T880-480q0 83-31.5
            156T763-197q-54 54-127 85.5T480-80Zm0-80q134 0 227-93t93-227q0-134-93-227t-227-93q-134 0-227
            93t-93 227q0 134 93 227t227 93Zm0-320Z
                """,
        )
    }

    val Error: ImageVector by lazy {
        symbol(
            "Error",
            mirrored = false,
            pathData =
                """
            M480-280q17 0 28.5-11.5T520-320q0-17-11.5-28.5T480-360q-17 0-28.5 11.5T440-320q0 17 11.5
            28.5T480-280Zm0-160q17 0 28.5-11.5T520-480v-160q0-17-11.5-28.5T480-680q-17 0-28.5
            11.5T440-640v160q0 17 11.5 28.5T480-440Zm0 360q-83
            0-156-31.5T197-197q-54-54-85.5-127T80-480q0-83 31.5-156T197-763q54-54 127-85.5T480-880q83 0 156
            31.5T763-763q54 54 85.5 127T880-480q0 83-31.5 156T763-197q-54 54-127 85.5T480-80Zm0-80q134 0
            227-93t93-227q0-134-93-227t-227-93q-134 0-227 93t-93 227q0 134 93 227t227 93Zm0-320Z
                """,
        )
    }

    val CheckCircle: ImageVector by lazy {
        symbol(
            "CheckCircle",
            mirrored = false,
            pathData =
                """
            m424-408-86-86q-11-11-28-11t-28 11q-11 11-11 28t11 28l114 114q12 12 28 12t28-12l226-226q11-11
            11-28t-11-28q-11-11-28-11t-28 11L424-408Zm56 328q-83
            0-156-31.5T197-197q-54-54-85.5-127T80-480q0-83 31.5-156T197-763q54-54 127-85.5T480-880q83 0 156
            31.5T763-763q54 54 85.5 127T880-480q0 83-31.5 156T763-197q-54 54-127 85.5T480-80Zm0-80q134 0
            227-93t93-227q0-134-93-227t-227-93q-134 0-227 93t-93 227q0 134 93 227t227 93Zm0-320Z
                """,
        )
    }

    val Check: ImageVector by lazy {
        symbol(
            "Check",
            mirrored = false,
            pathData =
                """
            m382-354 339-339q12-12 28-12t28 12q12 12 12 28.5T777-636L410-268q-12 12-28
            12t-28-12L182-440q-12-12-11.5-28.5T183-497q12-12 28.5-12t28.5 12l142 143Z
                """,
        )
    }

    val Edit: ImageVector by lazy {
        symbol(
            "Edit",
            mirrored = false,
            pathData =
                """
            M200-200h57l391-391-57-57-391 391v57Zm-40 80q-17 0-28.5-11.5T120-160v-97q0-16
            6-30.5t17-25.5l505-504q12-11 26.5-17t30.5-6q16 0 31 6t26 18l55 56q12 11 17.5 26t5.5 30q0 16-5.5
            30.5T817-647L313-143q-11 11-25.5 17t-30.5 6h-97Zm600-584-56-56 56 56Zm-141 85-28-29 57 57-29-28Z
                """,
        )
    }

    val DragHandle: ImageVector by lazy {
        symbol(
            "DragHandle",
            mirrored = false,
            pathData =
                """
            M200-360q-17 0-28.5-11.5T160-400q0-17 11.5-28.5T200-440h560q17 0 28.5 11.5T800-400q0 17-11.5
            28.5T760-360H200Zm0-160q-17 0-28.5-11.5T160-560q0-17 11.5-28.5T200-600h560q17 0 28.5
            11.5T800-560q0 17-11.5 28.5T760-520H200Z
                """,
        )
    }

    val ContentCopy: ImageVector by lazy {
        symbol(
            "ContentCopy",
            mirrored = false,
            pathData =
                """
            M360-240q-33 0-56.5-23.5T280-320v-480q0-33 23.5-56.5T360-880h360q33 0 56.5 23.5T800-800v480q0
            33-23.5 56.5T720-240H360Zm0-80h360v-480H360v480ZM200-80q-33 0-56.5-23.5T120-160v-520q0-17
            11.5-28.5T160-720q17 0 28.5 11.5T200-680v520h400q17 0 28.5 11.5T640-120q0 17-11.5
            28.5T600-80H200Zm160-240v-480 480Z
                """,
        )
    }

    val BrokenImage: ImageVector by lazy {
        symbol(
            "BrokenImage",
            mirrored = false,
            pathData =
                """
            M200-120q-33 0-56.5-23.5T120-200v-560q0-33 23.5-56.5T200-840h560q33 0 56.5 23.5T840-760v560q0
            33-23.5 56.5T760-120H200Zm40-337 132-132q12-12 28-12t28 12l132 132 132-132q12-12 28-12t28 12l12
            12v-183H200v263l40 40Zm-40 257h560v-264l-40-40-132 132q-12 12-28 12t-28-12L400-504 268-372q-12
            12-28 12t-28-12l-12-12v184Zm0 0v-264 80-376 560Z
                """,
        )
    }

    val List: ImageVector by lazy {
        symbol(
            "List",
            mirrored = true,
            pathData =
                """
            M320-600q-17 0-28.5-11.5T280-640q0-17 11.5-28.5T320-680h480q17 0 28.5 11.5T840-640q0 17-11.5
            28.5T800-600H320Zm0 160q-17 0-28.5-11.5T280-480q0-17 11.5-28.5T320-520h480q17 0 28.5
            11.5T840-480q0 17-11.5 28.5T800-440H320Zm0 160q-17 0-28.5-11.5T280-320q0-17
            11.5-28.5T320-360h480q17 0 28.5 11.5T840-320q0 17-11.5 28.5T800-280H320ZM160-600q-17
            0-28.5-11.5T120-640q0-17 11.5-28.5T160-680q17 0 28.5 11.5T200-640q0 17-11.5 28.5T160-600Zm0
            160q-17 0-28.5-11.5T120-480q0-17 11.5-28.5T160-520q17 0 28.5 11.5T200-480q0 17-11.5
            28.5T160-440Zm0 160q-17 0-28.5-11.5T120-320q0-17 11.5-28.5T160-360q17 0 28.5 11.5T200-320q0
            17-11.5 28.5T160-280Z
                """,
        )
    }

    val Keep: ImageVector by lazy {
        symbol(
            "Keep",
            mirrored = false,
            pathData =
                """
            M640-760v280l68 68q6 6 9 13.5t3 15.5v23q0 17-11.5 28.5T680-320H520v223q0 8-3 15.5T508-68l-14
            14q-6 6-14 6t-14-6l-14-14q-6-6-9-13.5T440-97v-223H280q-17 0-28.5-11.5T240-360v-23q0-8
            3-15.5t9-13.5l68-68v-280q-17 0-28.5-11.5T280-800q0-17 11.5-28.5T320-840h320q17 0 28.5
            11.5T680-800q0 17-11.5 28.5T640-760ZM354-400h252l-46-46v-314H400v314l-46 46Zm126 0Z
                """,
        )
    }

    val KeepOff: ImageVector by lazy {
        symbol(
            "KeepOff",
            mirrored = false,
            pathData =
                """
            M560-760H400v87L290-783q-5-5-7.5-11t-2.5-12q0-13 9-23.5t24-10.5h327q17 0 28.5 11.5T680-800q0
            16-14.5 22.5T640-760v240q0 17-11.5 28.5T600-480q-17
            0-28.5-11.5T560-520v-240ZM440-80v-240H296q-25 0-40-17.5T241-377q0-11
            4.5-22t14.5-21l60-60v-46L84-764q-11-11-11.5-27.5T84-820q11-11 28-11t28 11l679 679q12 12 11.5
            28.5T818-84q-12 11-28 11.5T762-84L526-320h-6v240q0 17-11.5 28.5T480-40q-17
            0-28.5-11.5T440-80Zm-86-320h92l-44-44-2-2-46 46Zm126-193Zm-78 149Z
                """,
        )
    }
}

/** The symbol whose path data is [pathData], 24dp across, mirrored in a right-to-left layout when [mirrored]. */
private fun symbol(
    name: String,
    mirrored: Boolean,
    pathData: String,
): ImageVector =
    ImageVector
        .Builder(
            name = name,
            defaultWidth = 24.dp,
            defaultHeight = 24.dp,
            viewportWidth = 960f,
            viewportHeight = 960f,
            autoMirror = mirrored,
        ).apply {
            // The symbols' paths sit in a box that runs from -960 to 0 vertically.
            group(translationY = 960f) {
                addPath(pathData = PathParser().parsePathString(pathData).toNodes(), fill = SolidColor(Color.Black))
            }
        }.build()
