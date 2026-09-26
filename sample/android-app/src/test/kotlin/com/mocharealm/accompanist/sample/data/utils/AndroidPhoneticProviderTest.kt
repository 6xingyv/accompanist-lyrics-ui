package com.mocharealm.accompanist.sample.data.utils

import kotlin.test.Test
import kotlin.test.assertEquals

class AndroidPhoneticProviderTest {

    @Test
    fun cjkSymbolsStayInPinyinBranch() {
        assertEquals("ni hao 。", AndroidPhoneticProvider.getPhonetic("你好。"))
    }

    @Test
    fun punctuationIsReturnedUnchanged() {
        val punctuation = "。？！～"

        assertEquals(punctuation, AndroidPhoneticProvider.getPhonetic(punctuation))
    }
}
