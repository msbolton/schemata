package com.github.msbolton.schemata.models

data class FieldModel(
    val fieldName: String,
    val fieldType: String,
    val nullable: Boolean = false
)
