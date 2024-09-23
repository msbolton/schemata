package com.github.msbolton.schemata

import SchemataBaseVisitor

class SchemataVisitor : SchemataBaseVisitor<Unit>() {

    override fun visitClassDeclaration(ctx: SchemataParser.ClassDeclarationContext) {
        val className = ctx.ID().text
        println("Class: $className")
        super.visitClassDeclaration(ctx)
    }

    override fun visitFieldDeclaration(ctx: SchemataParser.FieldDeclarationContext) {
        val fieldType = ctx.type().text
        val fieldName = ctx.ID().text
        println("  Field: $fieldName of type $fieldType")
        super.visitFieldDeclaration(ctx)
    }
}
