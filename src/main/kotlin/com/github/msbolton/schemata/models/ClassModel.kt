package com.github.msbolton.schemata.models

data class ClassModel(
    val className: String,
    val namespace: String = "",
    val fields: MutableList<FieldModel> = mutableListOf()
)
