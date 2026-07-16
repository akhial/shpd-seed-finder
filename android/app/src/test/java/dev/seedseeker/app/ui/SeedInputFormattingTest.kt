// SPDX-License-Identifier: GPL-3.0-or-later
package dev.seedseeker.app.ui

import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import org.junit.Assert.assertEquals
import org.junit.Test

class SeedInputFormattingTest {
    @Test
    fun typingAtEndKeepsCursorAfterAutoInsertedHyphens() {
        var value = TextFieldValue()

        "ABCDEFGHI".forEach { letter ->
            val cursor = value.selection.end
            val typed = value.text.substring(0, cursor) + letter + value.text.substring(cursor)
            value = formatSeedFieldValue(
                TextFieldValue(typed, selection = TextRange(cursor + 1)),
            )
        }

        assertEquals("ABC-DEF-GHI", value.text)
        assertEquals(TextRange(value.text.length), value.selection)
    }

    @Test
    fun formattingRemapsSelectionsByLogicalLetterPosition() {
        assertEquals(
            TextFieldValue("ABC-D", selection = TextRange(5)),
            formatSeedFieldValue(TextFieldValue("ABCD", selection = TextRange(4))),
        )
        assertEquals(
            TextFieldValue("ABC-DEF-GHI", selection = TextRange(5, 9)),
            formatSeedFieldValue(
                TextFieldValue("abc def ghi", selection = TextRange(5, 9)),
            ),
        )
    }
}
