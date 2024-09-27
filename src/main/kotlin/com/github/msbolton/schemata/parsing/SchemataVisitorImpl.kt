package com.github.msbolton.schemata.parsing

import com.github.msbolton.schemata.models.ClassModel
import com.github.msbolton.schemata.models.FieldModel

class SchemataVisitorImpl : SchemataBaseVisitor<ClassModel?>() {

    private var currentClassModel: ClassModel? = null
    private var currentNamespace: String = ""

    // Visit namespace declaration
    override fun visitNamespaceDeclaration(ctx: SchemataParser.NamespaceDeclarationContext): ClassModel? {
        currentNamespace = ctx.qualifiedName().text
        println("Namespace: $currentNamespace")
        return null
    }

    // Visit schema declaration and associate it with the current namespace
    override fun visitSchemaDeclaration(ctx: SchemataParser.SchemaDeclarationContext): ClassModel? {
        val className = ctx.ID().text
        currentClassModel = ClassModel(className, currentNamespace)

        // Visit the schema body to process fields
        visitSchemaBody(ctx.schemaBody())

        return currentClassModel
    }

    // Visit field declaration inside the schema
    override fun visitSchemaFieldDeclaration(ctx: SchemataParser.SchemaFieldDeclarationContext): ClassModel? {
        val fieldName = ctx.ID().text
        val fieldType = ctx.typeExpression().text

        // Check for nullable (?)
        val nullable = ctx.typeExpression().nullable() != null

        // Add the field to the current class model
        currentClassModel?.fields?.add(FieldModel(fieldName, fieldType, nullable))

        return currentClassModel
    }
}